//! A tiny static file server for previewing the built site. Local use only.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::thread;

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Maps a request target to a file under `root`, refusing anything that
/// could leave it.
pub fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next()?;
    let mut out = root.to_path_buf();
    for part in Path::new(path.trim_start_matches('/')).components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if path.contains('\\') || path.contains('\0') {
        return None;
    }
    if out.is_dir() {
        out.push("index.html");
    }
    Some(out)
}

fn handle(mut stream: TcpStream, root: &Path) -> io::Result<()> {
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let (status, ctype, body) = if method != "GET" && method != "HEAD" {
        (
            "405 Method Not Allowed",
            "text/plain",
            b"method not allowed".to_vec(),
        )
    } else {
        match resolve(root, target).and_then(|p| fs::read(&p).ok().map(|b| (p, b))) {
            Some((p, bytes)) => ("200 OK", content_type(&p), bytes),
            None => ("404 Not Found", "text/plain", b"not found".to_vec()),
        }
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if method != "HEAD" {
        stream.write_all(&body)?;
    }
    Ok(())
}

/// Serves `root` on localhost until the process is stopped.
pub fn serve(root: &Path, port: u16) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!(
        "Serving {} at http://127.0.0.1:{port}/ (Ctrl+C to stop)",
        root.display()
    );
    for stream in listener.incoming().flatten() {
        let root = root.to_path_buf();
        thread::spawn(move || {
            let _ = handle(stream, &root);
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_traversal() {
        let root = Path::new("site");
        assert!(resolve(root, "/../secret").is_none());
        assert!(resolve(root, "/a/../../b").is_none());
        assert!(resolve(root, "/a\\b").is_none());
        assert_eq!(resolve(root, "/x.html?y=1"), Some(root.join("x.html")));
    }
}
