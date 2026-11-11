mod config;
mod github;

use anyhow::{bail, Result};
use console::{style, Key, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

fn main() {
    if let Err(e) = run() {
        eprintln!("\n{} {e:#}", style("오류:").red().bold());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    println!("{}", style("gh-team — 팀 PR 현황 CLI").bold());
    loop {
        let cfg = config::load()?;
        println!(
            "현재 작업 그룹: {}",
            style(&cfg.current().name).cyan().bold()
        );
        let items = [
            "작업 그룹 설정",
            "구성원 작업 현황 보기",
            "구성원 리뷰 현황 보기",
            "레포별 열린 작업 보기",
            "종료",
        ];
        let sel = numbered_select("메뉴를 선택하세요", &items)?;
        let result = match sel {
            0 => group_menu(),
            1 => work_status(),
            2 => review_status(),
            3 => repo_status(),
            _ => break,
        };
        // 조회 중 오류가 나도 프로그램을 끝내지 않고 메뉴로 복귀
        if let Err(e) = result {
            eprintln!("\n{} {e:#}", style("오류:").red().bold());
        }
        println!();
    }
    Ok(())
}

/// 로딩 스피너 표시. 반환된 ProgressBar에 finish_and_clear()를 호출하면 사라진다.
fn spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("스피너 템플릿은 항상 유효"),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// 번호가 붙은 메뉴 선택.
/// 방향키(또는 j/k)로 이동 후 Enter, 혹은 숫자 키를 누르면 바로 실행된다.
fn numbered_select(prompt: &str, items: &[&str]) -> Result<usize> {
    let term = Term::stderr();
    term.hide_cursor()?;
    let header_lines = if prompt.is_empty() { 0 } else { 1 };
    let mut sel = 0usize;
    let picked = loop {
        if !prompt.is_empty() {
            term.write_line(&style(prompt).bold().to_string())?;
        }
        for (i, it) in items.iter().enumerate() {
            let line = format!("{}. {}", i + 1, it);
            if i == sel {
                term.write_line(&style(format!("> {line}")).cyan().to_string())?;
            } else {
                term.write_line(&format!("  {line}"))?;
            }
        }
        let key = term.read_key();
        term.clear_last_lines(items.len() + header_lines)?;
        let key = match key {
            Ok(k) => k,
            Err(e) => {
                let _ = term.show_cursor();
                return Err(e.into());
            }
        };
        match key {
            Key::ArrowDown | Key::Tab | Key::Char('j') => sel = (sel + 1) % items.len(),
            Key::ArrowUp | Key::BackTab | Key::Char('k') => {
                sel = (sel + items.len() - 1) % items.len()
            }
            Key::Enter => break sel,
            Key::Char(c) if c.is_ascii_digit() => {
                let n = (c as u8 - b'0') as usize;
                if (1..=items.len()).contains(&n) {
                    break n - 1;
                }
            }
            _ => {}
        }
    };
    term.show_cursor()?;
    if prompt.is_empty() {
        term.write_line(&style(items[picked]).cyan().to_string())?;
    } else {
        term.write_line(&format!(
            "{} {}",
            style(prompt).bold(),
            style(items[picked]).cyan()
        ))?;
    }
    Ok(picked)
}

enum MemberOrRepo {
    Member,
    Repo,
}

/// 구성원/레포 공용 CRUD 메뉴
fn list_menu(label: &str, kind: MemberOrRepo) -> Result<()> {
    loop {
        let cfg = config::load()?;
        let items: &Vec<String> = match kind {
            MemberOrRepo::Member => &cfg.current().members,
            MemberOrRepo::Repo => &cfg.current().repos,
        };

        println!(
            "\n{}",
            style(format!("[{}] 현재 {label} 목록", cfg.current().name)).bold()
        );
        if items.is_empty() {
            println!("  (없음)");
        }
        for it in items {
            println!("  • {it}");
        }

        let sel = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("{label} 설정"))
            .items(&["추가", "삭제", "뒤로 가기"])
            .default(0)
            .interact()?;

        match sel {
            0 => {
                let prompt = match kind {
                    MemberOrRepo::Member => "GitHub 유저명",
                    MemberOrRepo::Repo => "레포 이름 (owner/repo 형식)",
                };
                let input: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .interact_text()?;
                let value = input.trim().to_string();

                if value.is_empty() {
                    println!("{}", style("빈 값은 추가할 수 없습니다.").yellow());
                    continue;
                }
                if let MemberOrRepo::Repo = kind {
                    if value.split('/').filter(|s| !s.is_empty()).count() != 2 {
                        println!(
                            "{}",
                            style("레포는 owner/repo 형식이어야 합니다. 예) rust-lang/rust")
                                .yellow()
                        );
                        continue;
                    }
                }

                let mut cfg = cfg;
                let list = match kind {
                    MemberOrRepo::Member => &mut cfg.current_mut().members,
                    MemberOrRepo::Repo => &mut cfg.current_mut().repos,
                };
                if list.contains(&value) {
                    println!("{}", style("이미 등록되어 있습니다.").yellow());
                } else {
                    list.push(value);
                    config::save(&cfg)?;
                    println!("{}", style("추가되었습니다.").green());
                }
            }
            1 => {
                if items.is_empty() {
                    println!("{}", style(format!("삭제할 {label}이(가) 없습니다.")).yellow());
                    continue;
                }
                let idx = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!("삭제할 {label}"))
                    .items(items)
                    .default(0)
                    .interact()?;
                let target = items[idx].clone();
                let ok = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!("'{target}' 을(를) 삭제할까요?"))
                    .default(false)
                    .interact()?;
                if ok {
                    let mut cfg = cfg;
                    let list = match kind {
                        MemberOrRepo::Member => &mut cfg.current_mut().members,
                        MemberOrRepo::Repo => &mut cfg.current_mut().repos,
                    };
                    list.remove(idx);
                    config::save(&cfg)?;
                    println!("{}", style("삭제되었습니다.").green());
                }
            }
            _ => break,
        }
    }
    Ok(())
}

/// 작업 그룹 선택/추가/이름 수정/삭제 메뉴
fn group_menu() -> Result<()> {
    loop {
        let mut cfg = config::load()?;
        println!("\n{}", style("작업 그룹 목록").bold());
        for (i, g) in cfg.groups.iter().enumerate() {
            let marker = if i == cfg.selected { "✔" } else { " " };
            println!(
                "  {} {} (구성원 {}명, 레포 {}개)",
                style(marker).green(),
                g.name,
                g.members.len(),
                g.repos.len()
            );
        }
        let names: Vec<String> = cfg.groups.iter().map(|g| g.name.clone()).collect();
        let name_refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();

        let sel = numbered_select(
            "작업 그룹 설정",
            &[
                "선택",
                "추가",
                "이름 수정",
                "삭제",
                "구성원 설정",
                "레포 설정",
                "뒤로 가기",
            ],
        )?;
        match sel {
            0 => {
                let idx = numbered_select("사용할 작업 그룹", &name_refs)?;
                cfg.selected = idx;
                config::save(&cfg)?;
                println!(
                    "{}",
                    style(format!("'{}' 그룹을 선택했습니다.", names[idx])).green()
                );
                break;
            }
            1 => {
                let input: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("새 작업 그룹 이름")
                    .interact_text()?;
                let name = input.trim().to_string();
                if name.is_empty() {
                    println!("{}", style("빈 이름은 사용할 수 없습니다.").yellow());
                    continue;
                }
                if names.contains(&name) {
                    println!("{}", style("이미 같은 이름의 그룹이 있습니다.").yellow());
                    continue;
                }
                cfg.groups.push(config::Group {
                    name: name.clone(),
                    members: Vec::new(),
                    repos: Vec::new(),
                });
                cfg.selected = cfg.groups.len() - 1;
                config::save(&cfg)?;
                println!(
                    "{}",
                    style(format!("'{name}' 그룹을 추가하고 선택했습니다.")).green()
                );
            }
            2 => {
                let idx = numbered_select("이름을 수정할 그룹", &name_refs)?;
                let input: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("새 이름")
                    .interact_text()?;
                let name = input.trim().to_string();
                if name.is_empty() {
                    println!("{}", style("빈 이름은 사용할 수 없습니다.").yellow());
                    continue;
                }
                if names.contains(&name) {
                    println!("{}", style("이미 같은 이름의 그룹이 있습니다.").yellow());
                    continue;
                }
                cfg.groups[idx].name = name;
                config::save(&cfg)?;
                println!("{}", style("이름이 수정되었습니다.").green());
            }
            3 => {
                if cfg.groups.len() == 1 {
                    println!("{}", style("마지막 남은 그룹은 삭제할 수 없습니다.").yellow());
                    continue;
                }
                let idx = numbered_select("삭제할 그룹", &name_refs)?;
                let ok = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "'{}' 그룹을 삭제할까요? (구성원/레포 설정도 함께 삭제됩니다)",
                        names[idx]
                    ))
                    .default(false)
                    .interact()?;
                if ok {
                    cfg.groups.remove(idx);
                    if cfg.selected == idx {
                        cfg.selected = 0;
                    } else if cfg.selected > idx {
                        cfg.selected -= 1;
                    }
                    config::save(&cfg)?;
                    println!("{}", style("삭제되었습니다.").green());
                }
            }
            4 => list_menu("구성원", MemberOrRepo::Member)?,
            5 => list_menu("레포", MemberOrRepo::Repo)?,
            _ => break,
        }
    }
    Ok(())
}

fn ensure_ready(group: &config::Group) -> Result<()> {
    if group.members.is_empty() {
        bail!("등록된 구성원이 없습니다. 먼저 [구성원 설정]에서 추가하세요.");
    }
    if group.repos.is_empty() {
        bail!("등록된 레포가 없습니다. 먼저 [레포 설정]에서 추가하세요.");
    }
    Ok(())
}

/// OSC 8 시퀀스로 터미널 하이퍼링크 생성 (미지원 터미널에서는 텍스트만 표시됨)
fn link(text: &str, url: &str) -> String {
    if url.is_empty() {
        return text.to_string();
    }
    format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// macOS 클립보드에 서식 있는 텍스트(HTML)와 일반 텍스트를 함께 복사.
/// 슬랙 등 리치 텍스트 입력란에는 하이퍼링크가 유지되고,
/// 일반 텍스트만 받는 곳에는 plain 버전이 붙여넣어진다.
fn copy_rich_text_to_clipboard(html: &str, plain: &str) -> Result<()> {
    use std::io::Write;
    // «data HTML» 방식은 «class HTML» 형식으로 들어가 슬랙(Electron)이 읽지 못하므로
    // NSPasteboard로 public.html 형식을 직접 설정한다.
    const SCRIPT: &str = r#"use framework "AppKit"
on run argv
    set htmlText to item 1 of argv
    set plainText to item 2 of argv
    set pb to current application's NSPasteboard's generalPasteboard()
    pb's clearContents()
    pb's setString:htmlText forType:"public.html"
    pb's setString:plainText forType:"public.utf8-plain-text"
end run"#;
    let mut child = std::process::Command::new("osascript")
        .args(["-l", "AppleScript", "-", html, plain])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .expect("stdin은 piped로 열림")
        .write_all(SCRIPT.as_bytes())?;
    let status = child.wait()?;
    if !status.success() {
        bail!("클립보드 복사에 실패했습니다.");
    }
    Ok(())
}

/// HTML 특수문자 이스케이프
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// 리뷰 상태를 색이 입혀진 문자열로 변환
fn format_review_state(state: &github::ReviewState) -> String {
    use github::ReviewState::*;
    match state {
        Draft => style("초안(Draft)").yellow().to_string(),
        ChangesRequested => style("변경 요청됨").red().to_string(),
        InReview { approved: 0 } => style("리뷰중").color256(117).to_string(), // 하늘색
        InReview { approved } => {
            // 승인 수가 많을수록 진한 초록색
            let color = match approved {
                1 => 120, // 연한 초록
                2 => 77,  // 중간 초록
                _ => 46,  // 완전 초록
            };
            style(format!("{approved} 승인")).color256(color).to_string()
        }
        Waiting => style("리뷰 대기").dim().to_string(),
    }
}

/// "owner/repo" 에서 레포 이름만 추출
fn repo_name(repo: &str) -> &str {
    repo.split('/').nth(1).unwrap_or(repo)
}

fn repo_query(repos: &[String]) -> String {
    repos
        .iter()
        .map(|r| format!("repo:{r}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 메뉴 2: 구성원별 오픈 PR + 리뷰 상태
fn work_status() -> Result<()> {
    loop {
        work_status_view()?;
        println!();
        println!(
            "{}",
            style(format!("{}에 조회 됨", chrono::Local::now().format("%H:%M"))).dim()
        );
        if numbered_select("", &["새로고침", "나가기"])? == 1 {
            break;
        }
    }
    Ok(())
}

fn work_status_view() -> Result<()> {
    let cfg = config::load()?;
    let group = cfg.current();
    ensure_ready(group)?;
    let client = github::Client::new()?;
    let rq = repo_query(&group.repos);

    // 구성원별 검색과 PR별 리뷰 상태 조회를 모두 병렬로 실행한 뒤 한 번에 출력
    let sp = spinner("항목을 불러오고 있습니다...");
    let results: Vec<Result<Vec<(github::Pr, github::ReviewState)>>> = std::thread::scope(|s| {
        let handles: Vec<_> = group
            .members
            .iter()
            .map(|member| {
                let client = &client;
                let rq = &rq;
                s.spawn(move || {
                    let (_, mut open) = client.search_prs(
                        &format!("type:pr state:open author:{member} {rq}"),
                        None,
                        30,
                    )?;
                    open.sort_by_key(|pr| repo_name(&pr.repo).to_lowercase());
                    let states = std::thread::scope(|s2| {
                        let hs: Vec<_> = open
                            .iter()
                            .map(|pr| s2.spawn(move || client.review_state(pr)))
                            .collect();
                        hs.into_iter()
                            .map(|h| h.join().expect("리뷰 상태 조회 스레드 패닉"))
                            .collect::<Result<Vec<_>>>()
                    })?;
                    Ok(open.into_iter().zip(states).collect())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("구성원 조회 스레드 패닉"))
            .collect()
    });
    sp.finish_and_clear();

    for (member, result) in group.members.iter().zip(results) {
        println!("\n{}", style(member).cyan().bold());
        let prs = result?;
        if prs.is_empty() {
            println!("  열린 PR 없음");
        } else {
            for (pr, state) in &prs {
                println!(
                    "   • {} — {}",
                    link(
                        &format!("[{}] {}", repo_name(&pr.repo), pr.title),
                        &pr.html_url
                    ),
                    format_review_state(state)
                );
            }
        }
    }
    Ok(())
}

/// 메뉴 4: 레포별 열린 PR 목록 — 등록된 구성원이 만든 PR만 (리뷰 상태 없이)
fn repo_status() -> Result<()> {
    let cfg = config::load()?;
    let group = cfg.current();
    ensure_ready(group)?;
    let client = github::Client::new()?;

    // 레포별 검색을 병렬로 실행한 뒤 한 번에 출력
    let sp = spinner("항목을 불러오고 있습니다...");
    let results: Vec<Result<Vec<github::Pr>>> = std::thread::scope(|s| {
        let handles: Vec<_> = group
            .repos
            .iter()
            .map(|repo| {
                let client = &client;
                let members = &group.members;
                s.spawn(move || {
                    let (_, prs) = client.search_prs(
                        &format!("type:pr state:open repo:{repo}"),
                        None,
                        30,
                    )?;
                    Ok(prs
                        .into_iter()
                        .filter(|pr| {
                            members.iter().any(|m| m.eq_ignore_ascii_case(&pr.author))
                        })
                        .collect())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("레포 조회 스레드 패닉"))
            .collect()
    });
    sp.finish_and_clear();

    let mut share_html = String::new();
    let mut share_plain = String::new();
    for (repo, result) in group.repos.iter().zip(results) {
        let team_prs = result?;
        println!("\n{}", style(repo_name(repo)).cyan().bold());
        share_html.push_str(&format!(
            "<b>{}</b><br>\n<ul>\n",
            html_escape(repo_name(repo))
        ));
        share_plain.push_str(&format!("{}\n", repo_name(repo)));
        for pr in &team_prs {
            println!("   • {}", link(&pr.title, &pr.html_url));
            share_html.push_str(&format!(
                "<li><a href=\"{}\">{}</a></li>\n",
                pr.html_url,
                html_escape(&pr.title)
            ));
            share_plain.push_str(&format!("• {} — {}\n", pr.title, pr.html_url));
        }
        share_html.push_str("</ul>\n");
        share_plain.push('\n');
    }

    println!();
    let ok = numbered_select(
        "슬랙 공유용 텍스트를 클립보드에 복사할까요?",
        &["아니오", "네"],
    )? == 1;
    if ok {
        copy_rich_text_to_clipboard(
            &format!("<meta charset=\"utf-8\">\n{share_html}"),
            share_plain.trim_end(),
        )?;
        println!(
            "{}",
            style("복사되었습니다. 슬랙에 붙여넣으면 제목이 링크로 유지됩니다.").green()
        );
    }
    Ok(())
}

/// 메뉴 3: 구성원별 리뷰 요청받은 PR 개수
fn review_status() -> Result<()> {
    loop {
        review_status_view()?;
        println!();
        println!(
            "{}",
            style(format!("{}에 조회 됨", chrono::Local::now().format("%H:%M"))).dim()
        );
        if numbered_select("", &["새로고침", "나가기"])? == 1 {
            break;
        }
    }
    Ok(())
}

fn review_status_view() -> Result<()> {
    let cfg = config::load()?;
    let group = cfg.current();
    ensure_ready(group)?;
    let client = github::Client::new()?;
    let rq = repo_query(&group.repos);

    // 구성원별 검색 2건과 PR별 승인 여부 조회를 모두 병렬로 실행한 뒤 한 번에 출력
    let sp = spinner("항목을 불러오고 있습니다...");
    let results: Vec<Result<Vec<(github::Pr, github::MemberReview)>>> = std::thread::scope(|s| {
        let handles: Vec<_> = group
            .members
            .iter()
            .map(|member| {
                let client = &client;
                let rq = &rq;
                s.spawn(move || {
                    // 리뷰 요청받은 PR + 이미 리뷰를 남긴 PR (본인이 작성한 PR 제외)
                    let (requested, reviewed) = std::thread::scope(|s2| {
                        let h1 = s2.spawn(move || {
                            client.search_prs(
                                &format!("type:pr state:open review-requested:{member} {rq}"),
                                None,
                                30,
                            )
                        });
                        let h2 = s2.spawn(move || {
                            client.search_prs(
                                &format!(
                                    "type:pr state:open reviewed-by:{member} -author:{member} {rq}"
                                ),
                                None,
                                30,
                            )
                        });
                        (
                            h1.join().expect("검색 스레드 패닉"),
                            h2.join().expect("검색 스레드 패닉"),
                        )
                    });
                    let (_, mut prs) = requested?;
                    let (_, reviewed) = reviewed?;
                    for pr in reviewed {
                        if !prs.iter().any(|p| p.repo == pr.repo && p.number == pr.number) {
                            prs.push(pr);
                        }
                    }
                    prs.sort_by_key(|pr| repo_name(&pr.repo).to_lowercase());

                    let states = std::thread::scope(|s2| {
                        let hs: Vec<_> = prs
                            .iter()
                            .map(|pr| s2.spawn(move || client.member_review(pr, member)))
                            .collect();
                        hs.into_iter()
                            .map(|h| h.join().expect("리뷰 진행 상태 조회 스레드 패닉"))
                            .collect::<Result<Vec<_>>>()
                    })?;
                    Ok(prs.into_iter().zip(states).collect())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("구성원 조회 스레드 패닉"))
            .collect()
    });
    sp.finish_and_clear();

    println!("\n{}", style("리뷰 현황").bold());
    for (member, result) in group.members.iter().zip(results) {
        let prs = result?;
        println!(
            "\n{} — {}개",
            style(member).cyan().bold(),
            style(prs.len()).yellow().bold()
        );
        for (pr, state) in &prs {
            let status = match state {
                github::MemberReview::Approved => style("승인 완료").green(),
                github::MemberReview::InProgress => style("리뷰중").color256(117),
                github::MemberReview::NotStarted => style("진행 전").dim(),
            };
            println!(
                "   • {} — {}",
                link(
                    &format!("[{}] {}", repo_name(&pr.repo), pr.title),
                    &pr.html_url
                ),
                status
            );
        }
    }
    Ok(())
}
