use std::sync::Arc;

use percent_encoding::percent_decode_str;
use tauri::http::{
    header::{
        ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL, CONTENT_TYPE,
    },
    Method, Request, Response, StatusCode,
};
use tauri::{AppHandle, Manager};

use super::{ensure_package_webview_window_label, PluginHost};

const JAVASCRIPT_CONTENT_TYPE: &str = "text/javascript; charset=utf-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=utf-8";

pub(crate) fn respond_plugin_module_request(
    app_handle: &AppHandle,
    webview_label: &str,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let Some(host) = app_handle.try_state::<Arc<PluginHost>>() else {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "The plugin host is not ready.",
        );
    };

    respond_plugin_module_request_with(webview_label, request, |package_id, webview_id, path| {
        host.read_package_webview_module_text(package_id, webview_id, path)
    })
}

fn respond_plugin_module_request_with(
    webview_label: &str,
    request: Request<Vec<u8>>,
    read_module: impl FnOnce(&str, &str, &str) -> Result<String, String>,
) -> Response<Vec<u8>> {
    if request.method() == Method::OPTIONS {
        return response(StatusCode::NO_CONTENT, TEXT_CONTENT_TYPE, Vec::new());
    }
    if request.method() != Method::GET {
        return error_response(StatusCode::METHOD_NOT_ALLOWED, "Only GET is supported.");
    }

    let (package_id, webview_id, relative_path) =
        match parse_module_request_path(request.uri().path()) {
            Ok(parts) => parts,
            Err(message) => return error_response(StatusCode::BAD_REQUEST, &message),
        };
    if let Err(message) = ensure_package_webview_window_label(
        webview_label,
        &package_id,
        &webview_id,
        "load module files",
    ) {
        return error_response(StatusCode::FORBIDDEN, &message);
    }

    match read_module(&package_id, &webview_id, &relative_path) {
        Ok(source) => response(StatusCode::OK, JAVASCRIPT_CONTENT_TYPE, source.into_bytes()),
        Err(message) => error_response(StatusCode::NOT_FOUND, &message),
    }
}

fn parse_module_request_path(path: &str) -> Result<(String, String, String), String> {
    let mut segments = path.trim_start_matches('/').split('/');
    if segments.next() != Some("modules") {
        return Err("Unknown plugin module route.".to_string());
    }

    let package_id = decode_segment(segments.next(), "package id")?;
    let webview_id = decode_segment(segments.next(), "webview id")?;
    let module_segments = segments
        .map(|segment| decode_segment(Some(segment), "module path"))
        .collect::<Result<Vec<_>, _>>()?;
    if module_segments.is_empty()
        || module_segments
            .iter()
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err("Invalid plugin module path.".to_string());
    }
    let relative_path = module_segments.join("/");
    if !relative_path.ends_with(".js") {
        return Err("Plugin modules must be built .js files.".to_string());
    }

    Ok((package_id, webview_id, relative_path))
}

fn decode_segment(segment: Option<&str>, label: &str) -> Result<String, String> {
    let segment = segment.ok_or_else(|| format!("Missing {label}."))?;
    let decoded = percent_decode_str(segment)
        .decode_utf8()
        .map_err(|_| format!("Invalid UTF-8 in {label}."))?
        .into_owned();
    if decoded.is_empty() || decoded.contains('/') || decoded.contains('\\') {
        return Err(format!("Invalid {label}."));
    }
    Ok(decoded)
}

fn error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    response(status, TEXT_CONTENT_TYPE, message.as_bytes().to_vec())
}

fn response(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type)
        .header(ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS")
        .header(CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .expect("static plugin module protocol response must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(method: Method, path: &str) -> Request<Vec<u8>> {
        Request::builder()
            .method(method)
            .uri(format!("bakingrl-plugin://localhost{path}"))
            .body(Vec::new())
            .unwrap()
    }

    #[test]
    fn serves_javascript_only_to_its_declared_webview_window() {
        let response = respond_plugin_module_request_with(
            "plugin-webview-bakingrl_2eobs_2dgateway-obsGatewayConfig",
            request(
                Method::GET,
                "/modules/bakingrl.obs-gateway/obsGatewayConfig/dist/webviews/config.js?v=1",
            ),
            |package_id, webview_id, path| {
                assert_eq!(package_id, "bakingrl.obs-gateway");
                assert_eq!(webview_id, "obsGatewayConfig");
                assert_eq!(path, "dist/webviews/config.js");
                Ok("export default { mount() {} };".to_string())
            },
        );

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            JAVASCRIPT_CONTENT_TYPE
        );
        assert_eq!(
            String::from_utf8(response.into_body()).unwrap(),
            "export default { mount() {} };"
        );
    }

    #[test]
    fn rejects_requests_from_another_webview_window() {
        let response = respond_plugin_module_request_with(
            "plugin-webview-bakingrl_2eother-settings",
            request(
                Method::GET,
                "/modules/bakingrl.obs-gateway/obsGatewayConfig/dist/webviews/config.js",
            ),
            |_, _, _| panic!("the module reader must not run for a foreign window"),
        );

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn rejects_traversal_and_non_javascript_paths() {
        for path in [
            "/modules/package/settings/dist/%2e%2e/secret.js",
            "/modules/package/settings/dist/settings.css",
            "/modules/package/settings/dist%2fsettings.js",
        ] {
            let response = respond_plugin_module_request_with(
                "plugin-webview-package-settings",
                request(Method::GET, path),
                |_, _, _| panic!("the module reader must not run for an invalid path"),
            );
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        }
    }
}
