use super::*;
use crate::persistence::tests::TemporaryDirectory;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

const GIF: &[u8] =
    b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff,\0\0\0\0\x01\0\x01\0\0\x02\x02D\x01\0;";

fn server(responses: Vec<(&'static str, u16, Vec<u8>)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = thread::spawn(move || {
        for (path, status, body) in responses {
            let start = std::time::Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(5),
                            "request did not arrive: {path}"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("server error: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            let count = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..count]);
            assert!(
                request.starts_with(&format!("GET {path} HTTP/1.1\r\n")),
                "{request}"
            );
            write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    (url, handle)
}

#[tokio::test]
async fn website_icon_is_discovered() {
    let html = b"<link rel='icon' href='/app.gif'>";
    let (url, handle) = server(vec![
        ("/website", 200, html.to_vec()),
        ("/app.gif", 200, GIF.to_vec()),
    ]);
    assert_eq!(
        discover(&client().unwrap(), &format!("{url}/website"))
            .await
            .unwrap(),
        GIF
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn website_uses_relative_favicon_after_invalid_candidate() {
    let html = b"<link rel='shortcut icon' href='broken'><LINK HREF='icons/app.gif?x=1&amp;y=2' REL='icon'>";
    let (url, handle) = server(vec![
        ("/project/", 200, html.to_vec()),
        ("/project/broken", 200, b"<html>not an icon</html>".to_vec()),
        ("/project/icons/app.gif?x=1&y=2", 200, GIF.to_vec()),
    ]);
    assert_eq!(
        discover(&client().unwrap(), &format!("{url}/project/"))
            .await
            .unwrap(),
        GIF
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn failed_website_reports_an_error() {
    let (url, handle) = server(vec![
        ("/website", 404, Vec::new()),
        ("/favicon.ico", 404, Vec::new()),
    ]);
    assert!(discover(&client().unwrap(), &format!("{url}/website"))
        .await
        .is_err());
    handle.join().unwrap();
}

#[tokio::test]
async fn missing_icon_link_uses_origin_favicon() {
    let (url, handle) = server(vec![
        ("/project/page", 200, b"<html>no icon links</html>".to_vec()),
        ("/favicon.ico", 200, GIF.to_vec()),
    ]);
    assert_eq!(
        discover_website(&client().unwrap(), &format!("{url}/project/page"))
            .await
            .unwrap(),
        GIF
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn local_icon_is_copied_and_survives_original_removal() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("source.gif");
    let icons = directory.0.join("icons");
    fs::write(&source, GIF).unwrap();
    let acquired = acquire(&client().unwrap(), source.to_str().unwrap())
        .await
        .unwrap();
    let stored = save(&icons, &acquired).unwrap();
    assert_eq!(fs::read(&source).unwrap(), GIF);
    fs::remove_file(source).unwrap();
    assert_eq!(fs::read(&stored).unwrap(), GIF);
    assert_eq!(
        data_url(&icons, stored.to_str().unwrap()).unwrap(),
        format!("data:image/gif;base64,{}", base64(GIF))
    );
    assert_eq!(save(&icons, &acquired).unwrap(), stored);
}

#[tokio::test]
async fn unsupported_and_oversized_sources_are_rejected() {
    let directory = TemporaryDirectory::new();
    let path = directory.0.join("not-image.png");
    fs::write(&path, b"<html>error</html>").unwrap();
    assert!(acquire(&client().unwrap(), path.to_str().unwrap())
        .await
        .is_err());
    fs::write(&path, vec![0; IMAGE_LIMIT + 1]).unwrap();
    assert!(acquire(&client().unwrap(), path.to_str().unwrap())
        .await
        .is_err());
    assert!(acquire(&client().unwrap(), "relative.png").await.is_err());
    assert!(parse_url("https://user:password@example.com/image.png").is_err());
    assert!(save(&directory.0.join("icons"), b"invalid").is_err());
}

#[test]
fn data_url_rejects_paths_outside_icons_and_svg_active_content() {
    let directory = TemporaryDirectory::new();
    let icons = directory.0.join("icons");
    fs::create_dir(&icons).unwrap();
    let outside = directory.0.join("outside.gif");
    fs::write(&outside, GIF).unwrap();
    assert!(data_url(&icons, outside.to_str().unwrap()).is_err());
    assert!(
        image_format(b"<svg xmlns='http://www.w3.org/2000/svg'><path d='M0 0'/></svg>").is_ok()
    );
    for svg in [
        "<svg onload='alert(1)'></svg>",
        "<svg><script>alert(1)</script></svg>",
        "<svg><image href='https://example.com/a.png'/></svg>",
        "<!DOCTYPE svg><svg></svg>",
    ] {
        assert!(image_format(svg.as_bytes()).is_err(), "{svg}");
    }
}

#[test]
fn base64_handles_padding() {
    assert_eq!(base64(b""), "");
    assert_eq!(base64(b"f"), "Zg==");
    assert_eq!(base64(b"fo"), "Zm8=");
    assert_eq!(base64(b"foo"), "Zm9v");
    assert_eq!(base64(b"foobar"), "Zm9vYmFy");
}

#[test]
fn custom_icon_replaces_default_and_later_discovery_preserves_override() {
    let directory = TemporaryDirectory::new();
    let library_path = directory.0.join("library.json");
    let icons = directory.0.join("icons");
    let app = crate::models::InstalledApp {
        id: "example".into(),
        name: "Example".into(),
        file_path: "/example.AppImage".into(),
        source_path: None,
        file_name: "example.AppImage".into(),
        size_bytes: 20,
        executable: true,
        integrated: false,
        version_current: None,
        icon_path: None,
        desktop_id: None,
        launch_count: 3,
        last_launched: None,
        added_at: chrono::Utc::now(),
        file_created_at: None,
        launch_config: None,
        reviewed_digest: None,
        update_config: None,
    };
    persistence::mutate(&library_path, |library: &mut crate::store::Library| {
        library.insert(app.id.clone(), app.clone());
        Ok(())
    })
    .unwrap();
    let initial = persist_icon(&library_path, &icons, &app.id, GIF, false).unwrap();
    let custom = b"<svg xmlns='http://www.w3.org/2000/svg'><circle r='1'/></svg>";
    let updated = persist_icon(&library_path, &icons, &app.id, custom, true).unwrap();
    assert_ne!(updated.icon_path, initial.icon_path);
    assert_eq!(updated.launch_count, 3);
    assert_eq!(updated.added_at, app.added_at);
    assert_eq!(
        persist_icon(&library_path, &icons, &app.id, GIF, false).unwrap(),
        updated
    );
    assert!(persist_icon(&library_path, &icons, &app.id, b"invalid", true).is_err());
    let library: crate::store::Library = persistence::load(&library_path).unwrap();
    assert_eq!(library[&app.id], updated);
    assert_eq!(fs::read(updated.icon_path.unwrap()).unwrap(), custom);
    assert!(persist_icon(&library_path, &icons, "missing", GIF, true).is_err());
}

#[tokio::test]
async fn local_file_path_preserves_trailing_spaces_and_rejects_fifo() {
    let directory = TemporaryDirectory::new();
    let source = directory.0.join("icon.gif ");
    fs::write(&source, GIF).unwrap();
    assert_eq!(
        acquire(&client().unwrap(), source.to_str().unwrap())
            .await
            .unwrap(),
        GIF
    );
    let fifo = directory.0.join("fifo");
    assert!(std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    assert!(acquire(&client().unwrap(), fifo.to_str().unwrap())
        .await
        .is_err());
}

#[tokio::test]
async fn image_download_rejects_oversized_response_before_reading_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/image", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            IMAGE_LIMIT + 1
        )
        .unwrap();
    });
    assert!(acquire(&client().unwrap(), &url)
        .await
        .unwrap_err()
        .contains("size limit"));
    handle.join().unwrap();
}

#[test]
fn ordinary_svg_comments_namespaces_and_static_styles_are_supported() {
    let svg = br##"<?xml version="1.0" encoding="UTF-8"?>
<!-- Exported with Inkscape: https://inkscape.org/ -->
<svg xmlns="http://www.w3.org/2000/svg"
     xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
     xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
     width="32" height="32">
  <style>.icon { fill: #336699; stroke-width: 1; }</style>
  <path class="icon" d="M0 0L32 32"/>
</svg>"##;
    assert_eq!(image_format(svg).unwrap(), ("svg", "image/svg+xml"));
    for svg in [
        "<svg><style>@import 'https://example.com/style.css';</style></svg>",
        "<svg><style>.icon{fill:url(https://example.com/icon)}</style></svg>",
        "<svg><style>.icon{fill:url(#gradient)}</style></svg>",
        "<svg><style>@\\69mport 'external.css';</style></svg>",
        "<svg><style>.icon{fill:u&#114;l(external)}</style></svg>",
        "<svg xmlns:svg='http://www.w3.org/2000/svg'><svg:script>alert(1)</svg:script></svg>",
        "<!DOCTYPE svg [<!ENTITY external SYSTEM 'file:///tmp/icon'>]><svg>&external;</svg>",
    ] {
        assert!(image_format(svg.as_bytes()).is_err(), "{svg}");
    }
}
