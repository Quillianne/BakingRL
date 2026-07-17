use std::path::PathBuf;

use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Monitor, WebviewUrl, WebviewWindowBuilder,
};

use super::override_plugin_module_app_response;
use crate::plugin_package::manifest::PluginSurfaceOptionsV4;

const MIN_VISIBLE_LOGICAL_PIXELS: f64 = 64.0;

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SurfaceOpenOptions {
    pub position: Option<[f64; 2]>,
    pub size: Option<[f64; 2]>,
    pub screen: Option<String>,
}

impl SurfaceOpenOptions {
    pub fn parse(value: serde_json::Value) -> Result<Self, String> {
        let value = if value.is_null() {
            serde_json::json!({})
        } else {
            value
        };
        let options: Self = serde_json::from_value(value)
            .map_err(|error| format!("Invalid surface open options: {error}"))?;
        if options
            .position
            .is_some_and(|position| !position.iter().all(|value| value.is_finite()))
        {
            return Err("Surface position must contain finite coordinates.".to_string());
        }
        if options
            .size
            .is_some_and(|size| !size.iter().all(|value| value.is_finite() && *value > 0.0))
        {
            return Err("Surface size must contain positive finite dimensions.".to_string());
        }
        if options
            .screen
            .as_ref()
            .is_some_and(|screen| screen.trim().is_empty())
        {
            return Err("Surface screen cannot be empty.".to_string());
        }
        Ok(options)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceState {
    pub instance_id: String,
    pub screen: String,
    pub bounds: SurfaceBounds,
    pub scale_factor: f64,
    pub visible: bool,
}

pub struct SurfaceOpenResult {
    pub state: SurfaceState,
    pub diagnostic: Option<String>,
}

pub struct SurfaceOpenRequest<'a> {
    pub package_id: &'a str,
    pub surface_id: &'a str,
    pub route: &'a str,
    pub title: &'a str,
    pub default_size: [f64; 2],
    pub declaration: &'a PluginSurfaceOptionsV4,
    pub options: SurfaceOpenOptions,
}

pub fn open_surface(
    app_handle: &AppHandle,
    request: SurfaceOpenRequest<'_>,
) -> Result<SurfaceOpenResult, String> {
    let requested_screen = request
        .options
        .screen
        .as_deref()
        .or(request.declaration.default_screen.as_deref())
        .unwrap_or("primary");
    let (monitor, diagnostic) = select_monitor(app_handle, requested_screen)?;
    let scale_factor = monitor.scale_factor();
    let work_area = monitor.work_area();
    let work_x = work_area.position.x as f64 / scale_factor;
    let work_y = work_area.position.y as f64 / scale_factor;
    let work_width = work_area.size.width as f64 / scale_factor;
    let work_height = work_area.size.height as f64 / scale_factor;
    let [x, y] = request
        .options
        .position
        .or(request.declaration.default_position)
        .unwrap_or([0.0, 0.0]);
    let [width, height] = request.options.size.unwrap_or(request.default_size);
    let bounds = clamp_surface_bounds(
        work_width,
        work_height,
        SurfaceBounds {
            x,
            y,
            width,
            height,
        },
    )?;
    let absolute_x = work_x + bounds.x;
    let absolute_y = work_y + bounds.y;
    let label = surface_window_label(request.package_id, request.surface_id);
    let js_route = serde_json::to_string(request.route).map_err(|error| error.to_string())?;

    let window = if let Some(window) = app_handle.get_webview_window(&label) {
        window
            .eval(format!("window.location.href = {js_route};"))
            .map_err(|error| error.to_string())?;
        window
            .set_size(LogicalSize::new(bounds.width, bounds.height))
            .map_err(|error| error.to_string())?;
        window
            .set_position(LogicalPosition::new(absolute_x, absolute_y))
            .map_err(|error| error.to_string())?;
        window
            .set_resizable(request.declaration.resizable)
            .map_err(|error| error.to_string())?;
        window
            .set_always_on_top(request.declaration.always_on_top)
            .map_err(|error| error.to_string())?;
        window
            .set_ignore_cursor_events(request.declaration.click_through)
            .map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        window
    } else {
        let module_app_handle = app_handle.clone();
        let module_window_label = label.clone();
        let window = WebviewWindowBuilder::new(
            app_handle,
            &label,
            WebviewUrl::App(PathBuf::from(request.route)),
        )
        .on_web_resource_request(move |request, response| {
            override_plugin_module_app_response(
                &module_app_handle,
                &module_window_label,
                request,
                response,
            );
        })
        .title(request.title)
        .inner_size(bounds.width, bounds.height)
        .position(absolute_x, absolute_y)
        .decorations(false)
        .shadow(false)
        .resizable(request.declaration.resizable)
        .transparent(request.declaration.transparent)
        .always_on_top(request.declaration.always_on_top)
        .focused(false)
        .visible(true)
        .build()
        .map_err(|error| error.to_string())?;
        window
            .set_ignore_cursor_events(request.declaration.click_through)
            .map_err(|error| error.to_string())?;
        window
    };

    Ok(SurfaceOpenResult {
        state: SurfaceState {
            instance_id: label,
            screen: monitor
                .name()
                .cloned()
                .unwrap_or_else(|| "primary".to_string()),
            bounds,
            scale_factor,
            visible: window.is_visible().unwrap_or(true),
        },
        diagnostic,
    })
}

pub fn close_surface(
    app_handle: &AppHandle,
    package_id: &str,
    surface_id: &str,
) -> Result<bool, String> {
    let label = surface_window_label(package_id, surface_id);
    let Some(window) = app_handle.get_webview_window(&label) else {
        return Ok(false);
    };
    window.close().map_err(|error| error.to_string())?;
    Ok(true)
}

pub fn close_package_surfaces<'a>(
    app_handle: &AppHandle,
    package_id: &str,
    surface_ids: impl IntoIterator<Item = &'a str>,
) {
    for surface_id in surface_ids {
        if let Err(error) = close_surface(app_handle, package_id, surface_id) {
            tracing::warn!(
                "Unable to close plugin surface '{}/{}': {}",
                package_id,
                surface_id,
                error
            );
        }
    }
}

pub fn surface_window_label(package_id: &str, surface_id: &str) -> String {
    format!(
        "plugin-surface-{}-{}",
        hex::encode(package_id.as_bytes()),
        hex::encode(surface_id.as_bytes())
    )
}

fn select_monitor(
    app_handle: &AppHandle,
    requested_screen: &str,
) -> Result<(Monitor, Option<String>), String> {
    let monitors = app_handle
        .available_monitors()
        .map_err(|error| format!("Unable to list monitors: {error}"))?;
    let primary = app_handle
        .primary_monitor()
        .map_err(|error| format!("Unable to resolve the primary monitor: {error}"))?
        .or_else(|| monitors.first().cloned())
        .ok_or_else(|| "No monitor is available for plugin surface creation.".to_string())?;
    if requested_screen == "primary" {
        return Ok((primary, None));
    }
    if let Some(monitor) = monitors
        .into_iter()
        .find(|monitor| monitor.name().is_some_and(|name| name == requested_screen))
    {
        return Ok((monitor, None));
    }
    Ok((
        primary,
        Some(format!(
            "Surface requested unavailable screen '{requested_screen}'; using the primary screen."
        )),
    ))
}

fn clamp_surface_bounds(
    work_width: f64,
    work_height: f64,
    bounds: SurfaceBounds,
) -> Result<SurfaceBounds, String> {
    if !work_width.is_finite()
        || !work_height.is_finite()
        || work_width <= 0.0
        || work_height <= 0.0
    {
        return Err("Surface work area must have positive finite dimensions.".to_string());
    }
    if ![bounds.x, bounds.y, bounds.width, bounds.height]
        .iter()
        .all(|value| value.is_finite())
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return Err("Surface bounds must contain positive finite dimensions.".to_string());
    }
    let visible_width = MIN_VISIBLE_LOGICAL_PIXELS.min(bounds.width).min(work_width);
    let visible_height = MIN_VISIBLE_LOGICAL_PIXELS
        .min(bounds.height)
        .min(work_height);
    Ok(SurfaceBounds {
        x: bounds
            .x
            .clamp(visible_width - bounds.width, work_width - visible_width),
        y: bounds
            .y
            .clamp(visible_height - bounds.height, work_height - visible_height),
        ..bounds
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_options_are_strict_and_validate_bounds() {
        assert!(SurfaceOpenOptions::parse(serde_json::json!({
            "position": [10, 20],
            "size": [640, 360],
            "screen": "primary"
        }))
        .is_ok());
        assert!(SurfaceOpenOptions::parse(serde_json::json!({ "width": 640 })).is_err());
        assert!(SurfaceOpenOptions::parse(serde_json::json!({ "size": [0, 360] })).is_err());
    }

    #[test]
    fn keeps_at_least_sixty_four_pixels_visible() {
        let bounds = clamp_surface_bounds(
            1920.0,
            1080.0,
            SurfaceBounds {
                x: 4000.0,
                y: -4000.0,
                width: 800.0,
                height: 600.0,
            },
        )
        .unwrap();
        assert_eq!(bounds.x, 1856.0);
        assert_eq!(bounds.y, -536.0);
    }

    #[test]
    fn keeps_small_surfaces_fully_visible() {
        let bounds = clamp_surface_bounds(
            100.0,
            100.0,
            SurfaceBounds {
                x: -50.0,
                y: 100.0,
                width: 20.0,
                height: 30.0,
            },
        )
        .unwrap();
        assert_eq!(bounds.x, 0.0);
        assert_eq!(bounds.y, 70.0);
    }

    #[test]
    fn surface_labels_do_not_collapse_dots_and_dashes() {
        assert_ne!(
            surface_window_label("bakingrl.poc", "score"),
            surface_window_label("bakingrl-poc", "score")
        );
    }
}
