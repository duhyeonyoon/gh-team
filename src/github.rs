use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::collections::HashMap;

const API: &str = "https://api.github.com";

#[derive(Debug, Clone)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub repo: String, // "owner/name"
    pub author: String,
    pub html_url: String,
    pub draft: bool,
}

/// 특정 구성원의 PR 리뷰 진행 상태
#[derive(Debug, Clone, Copy)]
pub enum MemberReview {
    /// 승인 완료
    Approved,
    /// 코멘트 등 리뷰 흔적은 있으나 승인 전
    InProgress,
    /// 아직 리뷰를 시작하지 않음
    NotStarted,
}

/// PR 리뷰 상태
#[derive(Debug, Clone, Copy)]
pub enum ReviewState {
    Draft,
    ChangesRequested,
    /// 리뷰 진행 중 (approved = 현재까지 승인 수)
    InReview { approved: usize },
    Waiting,
}

pub struct Client {
    token: String,
    agent: ureq::Agent,
}

impl Client {
    pub fn new() -> Result<Self> {
        let token = std::env::var("GITHUB_TOKEN").map_err(|_| {
            anyhow!(
                "GITHUB_TOKEN 환경변수가 설정되어 있지 않습니다.\n  \
                 예) export GITHUB_TOKEN=ghp_xxxx 실행 후 다시 시도하세요."
            )
        })?;
        Ok(Self {
            token,
            agent: ureq::AgentBuilder::new().build(),
        })
    }

    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<Value> {
        let mut req = self
            .agent
            .get(url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("User-Agent", "gh-team-cli");
        for (k, v) in params {
            req = req.query(k, v);
        }
        match req.call() {
            Ok(resp) => Ok(resp.into_json()?),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                match code {
                    401 => bail!("인증 실패(401): GITHUB_TOKEN이 유효한지 확인하세요."),
                    403 | 429 => bail!(
                        "권한 부족 또는 요청 제한 (HTTP {code}). \
                         토큰 권한을 확인하거나 잠시 후 다시 시도하세요.\n{body}"
                    ),
                    422 => bail!(
                        "잘못된 검색 조건 (HTTP 422). \
                         레포 이름(owner/repo)과 유저명을 확인하세요.\n{body}"
                    ),
                    _ => bail!("GitHub API 오류 (HTTP {code}): {body}"),
                }
            }
            Err(e) => bail!("네트워크 오류: {e}"),
        }
    }

    /// 검색 API로 PR 목록 조회. (총 개수, PR 목록) 반환.
    pub fn search_prs(
        &self,
        query: &str,
        sort: Option<&str>,
        per_page: u32,
    ) -> Result<(u64, Vec<Pr>)> {
        let pp = per_page.to_string();
        let mut params: Vec<(&str, &str)> =
            vec![("q", query), ("per_page", pp.as_str()), ("order", "desc")];
        if let Some(s) = sort {
            params.push(("sort", s));
        }
        let v = self.get(&format!("{API}/search/issues"), &params)?;

        let total = v["total_count"].as_u64().unwrap_or(0);
        let mut prs = Vec::new();
        if let Some(items) = v["items"].as_array() {
            for item in items {
                // repository_url: "https://api.github.com/repos/{owner}/{repo}"
                let repo = item["repository_url"]
                    .as_str()
                    .and_then(|u| u.split("/repos/").nth(1))
                    .unwrap_or("?")
                    .to_string();
                prs.push(Pr {
                    number: item["number"].as_u64().unwrap_or(0),
                    title: item["title"].as_str().unwrap_or("(제목 없음)").to_string(),
                    author: item["user"]["login"].as_str().unwrap_or("").to_string(),
                    html_url: item["html_url"].as_str().unwrap_or("").to_string(),
                    draft: item["draft"].as_bool().unwrap_or(false),
                    repo,
                });
            }
        }
        Ok((total, prs))
    }

    /// 특정 유저의 이 PR에 대한 리뷰 진행 상태 (시간순으로 마지막 유효 상태 기준)
    pub fn member_review(&self, pr: &Pr, user: &str) -> Result<MemberReview> {
        let reviews = self.get(
            &format!("{API}/repos/{}/pulls/{}/reviews", pr.repo, pr.number),
            &[("per_page", "100")],
        )?;
        let mut approved = false;
        let mut has_activity = false;
        if let Some(list) = reviews.as_array() {
            for r in list {
                let login = r["user"]["login"].as_str().unwrap_or("");
                if !login.eq_ignore_ascii_case(user) {
                    continue;
                }
                has_activity = true;
                match r["state"].as_str().unwrap_or("") {
                    "APPROVED" => approved = true,
                    "CHANGES_REQUESTED" | "DISMISSED" => approved = false,
                    _ => {}
                }
            }
        }
        Ok(if approved {
            MemberReview::Approved
        } else if has_activity {
            MemberReview::InProgress
        } else {
            MemberReview::NotStarted
        })
    }

    /// PR의 리뷰 상태를 반환.
    pub fn review_state(&self, pr: &Pr) -> Result<ReviewState> {
        if pr.draft {
            return Ok(ReviewState::Draft);
        }
        let base = format!("{API}/repos/{}/pulls/{}", pr.repo, pr.number);
        let reviews = self.get(&format!("{base}/reviews"), &[("per_page", "100")])?;
        // 리뷰어별 마지막 유효 상태만 유지 (시간순 배열)
        let mut latest: HashMap<String, String> = HashMap::new();
        let mut has_any_review = false;
        if let Some(list) = reviews.as_array() {
            for r in list {
                let user = r["user"]["login"].as_str().unwrap_or("").to_string();
                // 작성자 본인의 코멘트 리뷰는 리뷰 활동으로 치지 않음
                if !user.eq_ignore_ascii_case(&pr.author) {
                    has_any_review = true;
                }
                match r["state"].as_str().unwrap_or("") {
                    s @ ("APPROVED" | "CHANGES_REQUESTED") => {
                        latest.insert(user, s.to_string());
                    }
                    "DISMISSED" => {
                        latest.remove(&user);
                    }
                    _ => {}
                }
            }
        }

        let approved = latest.values().filter(|s| *s == "APPROVED").count();
        let state = if latest.values().any(|s| s == "CHANGES_REQUESTED") {
            ReviewState::ChangesRequested
        } else if approved > 0 || has_any_review {
            // 승인 수와 함께 진행 중으로 표시 (승인 0이면 코멘트 등 활동만 있는 상태)
            ReviewState::InReview { approved }
        } else {
            // 리뷰어 지정 여부와 관계없이, 실제 리뷰가 없으면 리뷰 대기
            ReviewState::Waiting
        };
        Ok(state)
    }
}
