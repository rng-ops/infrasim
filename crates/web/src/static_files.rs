//! Static file serving

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

/// Static file handler
pub struct StaticFiles {
    // In production, would cache files or serve from disk
}

impl StaticFiles {
    pub fn new() -> Self {
        Self {}
    }

    /// Serve a static file
    pub async fn serve(&self, path: &str) -> Response {
        // For MVP, we embed essential noVNC files
        // In production, you would serve from disk or CDN
        
        let content_type = guess_content_type(path);
        
        match path {
            // Core noVNC files would be served here
            "core/rfb.js" => serve_embedded(RFB_JS, content_type),
            "core/util/logging.js" => serve_embedded(LOGGING_JS, content_type),
            _ => (StatusCode::NOT_FOUND, "File not found").into_response(),
        }
    }
}

impl Default for StaticFiles {
    fn default() -> Self {
        Self::new()
    }
}

fn guess_content_type(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

fn serve_embedded(content: &'static str, content_type: &'static str) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type)],
        content,
    )
        .into_response()
}

// Minimal embedded JavaScript stubs
// In production, download full noVNC from https://github.com/novnc/noVNC

const RFB_JS: &str = r#"
// Minimal RFB stub - use full noVNC for production
export default class RFB {
    constructor(target, url, options = {}) {
        this.target = target;
        this.url = url;
        this.options = options;
        this._connected = false;
        
        console.log('RFB: Connecting to', url);
        this.connect();
    }
    
    connect() {
        // Stub implementation
        console.log('RFB: Use full noVNC library for complete VNC support');
    }
    
    disconnect() {
        this._connected = false;
    }
    
    sendCredentials(credentials) {
        console.log('RFB: sendCredentials');
    }
    
    sendKey(keysym, code, down) {
        console.log('RFB: sendKey', keysym, down);
    }
    
    sendCtrlAltDel() {
        console.log('RFB: sendCtrlAltDel');
    }
    
    machineShutdown() {
        console.log('RFB: machineShutdown');
    }
    
    machineReboot() {
        console.log('RFB: machineReboot');
    }
    
    clipboardPasteFrom(text) {
        console.log('RFB: clipboardPasteFrom');
    }
    
    get capabilities() {
        return { power: false, resize: true };
    }
}
"#;

const LOGGING_JS: &str = r#"
// Minimal logging stub
export function initLogging(level = 'warn') {
    console.log('Logging initialized at level:', level);
}

export function Debug(msg) { console.debug(msg); }
export function Info(msg) { console.info(msg); }
export function Warn(msg) { console.warn(msg); }
export function Error(msg) { console.error(msg); }
"#;
