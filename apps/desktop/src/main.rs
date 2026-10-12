//! Optional OS-decorated window. Same local server, zero invoke commands or plugins.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod workspace;
use clap::Parser;
use job_harness_protocol::dashboard::{DashboardState, router};
use std::{net::TcpListener, path::PathBuf};
#[cfg(target_os = "macos")]
use tauri::Manager;
use tauri::{WebviewUrl, WebviewWindowBuilder, webview::NewWindowResponse};
use url::Url;

#[derive(Parser)]
#[command(
    version,
    about = "Open a private Job Harness workspace in a native window"
)]
struct Options {
    /// Existing initialized workspace. No workspace is silently created.
    #[arg(long)]
    root: Option<PathBuf>,
    /// Enable named, user-reviewed local tracking/note writes. No external action routes.
    #[arg(long)]
    allow_local_writes: bool,
    /// Open the native document window without its named local editing actions.
    #[arg(long, conflicts_with = "allow_local_writes")]
    read_only: bool,
}

fn same_local_origin(target: &Url, local: &Url) -> bool {
    target.origin() == local.origin() && target.username().is_empty() && target.password().is_none()
}
fn browser_link(url: &Url) -> bool {
    matches!(url.scheme(), "https" | "http")
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some()
}

fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    let Some(root) = workspace::select(options.root)? else {
        return Ok(());
    };
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let state = DashboardState::with_native_policy(&root, port, !options.read_only)?;
    let menu_state = state.clone();
    let shutdown_state = state.clone();
    let origin: Url = format!("http://127.0.0.1:{port}/").parse()?;
    let launch: Url = state.launch_url()?.parse()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = runtime.spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener)?;
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
    });
    let local = origin.clone();
    let app = tauri::Builder::default()
        .setup(move |app| {
            let menu = tauri::menu::Menu::default(app.handle())?;
            let review = tauri::menu::CheckMenuItem::with_id(
                app,
                "review-local",
                "Read only",
                true,
                options.read_only,
                None::<&str>,
            )?;
            let workspace_menu =
                tauri::menu::Submenu::with_items(app, "Workspace", true, &[&review])?;
            menu.append(&workspace_menu)?;
            app.set_menu(menu)?;
            app.on_menu_event(move |_, event| {
                if event.id().as_ref() == "review-local"
                    && let Ok(enabled) = review.is_checked()
                {
                    let _ = menu_state.set_native_review_mode(!enabled);
                }
            });
            WebviewWindowBuilder::new(app, "workspace", WebviewUrl::External(launch.clone()))
                .title("Job Harness")
                .inner_size(1240.0, 740.0)
                .min_inner_size(680.0, 520.0)
                .decorations(true)
                .incognito(true)
                // Keep card gestures in the frontend; WebView2 otherwise replaces HTML5 drop handling.
                .disable_drag_drop_handler()
                .on_navigation(move |target| same_local_origin(target, &local))
                .on_new_window(|url, _features| {
                    if browser_link(&url) {
                        // A clicked external link opens in the user's browser; never in this webview.
                        let _ = open::that_detached(url.as_str());
                    }
                    NewWindowResponse::Deny
                })
                .on_download(|_, _| false)
                .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
                .on_document_title_changed(|window, title| {
                    let _ = window.set_title(&title);
                })
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())?;
    let _ = app.run_return(|app, event| {
        #[cfg(target_os = "macos")]
        if matches!(event, tauri::RunEvent::Reopen { .. })
            && let Some(window) = app.get_webview_window("workspace")
        {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
    shutdown_state.close_notifications();
    let _ = stop.send(());
    runtime.block_on(async {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), task).await;
    });
    // Dropping the runtime closes any remaining local requests; no child agent is spawned.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_document_edits_default_on_and_read_only_is_explicit() {
        assert!(!Options::try_parse_from(["desktop"]).unwrap().read_only);
        assert!(
            Options::try_parse_from(["desktop", "--read-only"])
                .unwrap()
                .read_only
        );
        assert!(
            Options::try_parse_from(["desktop", "--read-only", "--allow-local-writes"]).is_err()
        );
    }
    #[test]
    fn webview_navigation_stays_in_exact_loopback_origin() {
        let local = Url::parse("http://127.0.0.1:12345/").unwrap();
        for allowed in [
            "http://127.0.0.1:12345/#knowledge",
            "http://127.0.0.1:12345/api/overview",
        ] {
            assert!(same_local_origin(&Url::parse(allowed).unwrap(), &local));
        }
        for denied in [
            "https://example.com/",
            "http://localhost:12345/",
            "http://127.0.0.1:12346/",
            "file:///etc/passwd",
            "http://user@127.0.0.1:12345/",
        ] {
            assert!(!same_local_origin(&Url::parse(denied).unwrap(), &local));
        }
    }
    #[test]
    fn external_links_allow_only_web_urls_without_credentials() {
        assert!(browser_link(
            &Url::parse("https://example.com/job").unwrap()
        ));
        for denied in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://user:password@example.com/",
        ] {
            assert!(!browser_link(&Url::parse(denied).unwrap()));
        }
    }
}
