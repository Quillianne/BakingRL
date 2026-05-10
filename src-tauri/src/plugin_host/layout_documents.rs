use crate::models::{
    OverlayItem, OverlayLayer, OverlayLayout, OverlayLayoutsFile, PageBackground, PageItem,
    PageLayer, PageLayout, PageSettings, PagesFile,
};

use super::{now_ms, unique_id};

pub(super) fn ensure_active_layout_ids(file: &mut OverlayLayoutsFile) {
    if !file
        .layouts
        .iter()
        .any(|layout| layout.id == file.active_layout_id)
    {
        file.active_layout_id = file
            .layouts
            .first()
            .map(|layout| layout.id.clone())
            .unwrap_or_default();
    }
    if !file
        .layouts
        .iter()
        .any(|layout| layout.id == file.stream_layout_id)
    {
        file.stream_layout_id = file
            .layouts
            .first()
            .map(|layout| layout.id.clone())
            .unwrap_or_default();
    }
}

pub(super) fn normalize_overlay_layouts(file: &mut OverlayLayoutsFile) {
    for layout in &mut file.layouts {
        normalize_overlay_layout(layout, false);
    }
}

pub(super) fn normalize_overlay_layout(layout: &mut OverlayLayout, touch_updated: bool) {
    let now = now_ms();
    if layout.id.trim().is_empty() {
        layout.id = unique_id("overlay");
    }
    if layout.name.trim().is_empty() {
        layout.name = "Untitled Layout".to_string();
    }
    if layout.width <= 0.0 {
        layout.width = 1920.0;
    }
    if layout.height <= 0.0 {
        layout.height = 1080.0;
    }
    if layout.created_at_ms == 0 {
        layout.created_at_ms = now;
    }
    if layout.updated_at_ms == 0 || touch_updated {
        layout.updated_at_ms = now;
    }
    if layout.layers.is_empty() {
        let mut layers = default_layers();
        if !layout.items.is_empty() {
            layers[0].items = std::mem::take(&mut layout.items);
        }
        layout.layers = layers;
    } else if !layout.items.is_empty() {
        let legacy_items = std::mem::take(&mut layout.items);
        match layout
            .layers
            .iter_mut()
            .find(|layer| layer.kind == "normal")
        {
            Some(layer) => layer.items.extend(legacy_items),
            None => layout.layers.push(OverlayLayer {
                id: unique_id("layer"),
                name: "Main".to_string(),
                kind: "normal".to_string(),
                visible: true,
                locked: false,
                order: 0,
                items: legacy_items,
            }),
        }
    }

    layout.layers.sort_by(|a, b| {
        if a.kind == "event" && b.kind != "event" {
            std::cmp::Ordering::Greater
        } else if a.kind != "event" && b.kind == "event" {
            std::cmp::Ordering::Less
        } else {
            a.order.cmp(&b.order)
        }
    });

    let mut event_seen = false;
    for (index, layer) in layout.layers.iter_mut().enumerate() {
        if layer.id.trim().is_empty() {
            layer.id = unique_id("layer");
        }
        if layer.name.trim().is_empty() {
            layer.name = if layer.kind == "event" {
                "Events".to_string()
            } else {
                "Layer".to_string()
            };
        }
        if layer.kind != "event" {
            layer.kind = "normal".to_string();
        } else if event_seen {
            layer.kind = "normal".to_string();
        } else {
            event_seen = true;
        }
        layer.order = index as i32;
        for item in &mut layer.items {
            normalize_overlay_item(item);
        }
    }

    if !event_seen {
        layout.layers.push(OverlayLayer {
            id: unique_id("layer"),
            name: "Events".to_string(),
            kind: "event".to_string(),
            visible: true,
            locked: false,
            order: layout.layers.len() as i32,
            items: Vec::new(),
        });
    }
}

fn normalize_overlay_item(item: &mut OverlayItem) {
    if item.name.trim().is_empty() {
        item.name = item.export_name.clone();
    }
    item.opacity = item.opacity.clamp(0.0, 1.0);
}

pub(super) fn rekey_overlay_layout_contents(layout: &mut OverlayLayout) {
    let stamp = now_ms();
    for (layer_index, layer) in layout.layers.iter_mut().enumerate() {
        layer.id = format!("layer-{stamp}-{layer_index}");
        for (item_index, item) in layer.items.iter_mut().enumerate() {
            item.id = format!("item-{stamp}-{layer_index}-{item_index}");
        }
    }
    for (item_index, item) in layout.items.iter_mut().enumerate() {
        item.id = format!("item-{stamp}-legacy-{item_index}");
    }
}

pub(super) fn normalize_pages(file: &mut PagesFile) {
    for page in &mut file.pages {
        normalize_page(page, false);
    }
    file.pages.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.name.cmp(&b.name))
    });
}

pub(super) fn normalize_page(page: &mut PageLayout, touch_updated: bool) {
    let now = now_ms();
    if page.id.trim().is_empty() {
        page.id = unique_id("page");
    }
    if page.name.trim().is_empty() {
        page.name = "Untitled Page".to_string();
    }
    if page.width <= 0.0 {
        page.width = 1440.0;
    }
    if page.height <= 0.0 {
        page.height = 900.0;
    }
    if page.background.kind != "image" {
        page.background.kind = "color".to_string();
    }
    if page.background.color.trim().is_empty() {
        page.background.color = "#0f172a".to_string();
    }
    if page.background.fit != "contain" && page.background.fit != "stretch" {
        page.background.fit = "cover".to_string();
    }
    if page.settings.open_target != "window" {
        page.settings.open_target = "app".to_string();
    }
    if page.created_at_ms == 0 {
        page.created_at_ms = now;
    }
    if page.updated_at_ms == 0 || touch_updated {
        page.updated_at_ms = now;
    }
    if page.layers.is_empty() {
        page.layers = default_page_layers();
    }
    page.layers.sort_by(|a, b| a.order.cmp(&b.order));
    for (index, layer) in page.layers.iter_mut().enumerate() {
        if layer.id.trim().is_empty() {
            layer.id = unique_id("layer");
        }
        if layer.name.trim().is_empty() {
            layer.name = "Layer".to_string();
        }
        layer.kind = "normal".to_string();
        layer.order = index as i32;
        for item in &mut layer.items {
            normalize_page_item(item);
        }
    }
}

fn normalize_page_item(item: &mut PageItem) {
    if item.kind.trim().is_empty() {
        item.kind = if item
            .package_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && item
                .export_name
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        {
            "visual".to_string()
        } else {
            "text".to_string()
        };
    }
    if item.kind != "visual" && item.kind != "text" && item.kind != "image" && item.kind != "shape"
    {
        item.kind = "visual".to_string();
    }
    if item.kind == "visual" {
        item.package_id = item
            .package_id
            .as_ref()
            .map(|value| value.trim().to_string());
        item.export_name = item
            .export_name
            .as_ref()
            .map(|value| value.trim().to_string());
    } else {
        item.package_id = None;
        item.export_name = None;
    }
    if item.name.trim().is_empty() {
        item.name = match item.kind.as_str() {
            "text" => "Text".to_string(),
            "image" => "Image".to_string(),
            "shape" => "Shape".to_string(),
            _ => item
                .export_name
                .clone()
                .unwrap_or_else(|| "Visual".to_string()),
        };
    }
    if item.width <= 0.0 {
        item.width = 320.0;
    }
    if item.height <= 0.0 {
        item.height = 120.0;
    }
    item.opacity = item.opacity.clamp(0.0, 1.0);
    if !item.settings.is_object() {
        item.settings = serde_json::json!({});
    }
}

pub(super) fn default_layers() -> Vec<OverlayLayer> {
    vec![
        OverlayLayer {
            id: "main".to_string(),
            name: "Main".to_string(),
            kind: "normal".to_string(),
            visible: true,
            locked: false,
            order: 0,
            items: Vec::new(),
        },
        OverlayLayer {
            id: "events".to_string(),
            name: "Events".to_string(),
            kind: "event".to_string(),
            visible: true,
            locked: false,
            order: 1,
            items: Vec::new(),
        },
    ]
}

pub(super) fn default_page_layers() -> Vec<PageLayer> {
    vec![PageLayer {
        id: "content".to_string(),
        name: "Content".to_string(),
        kind: "normal".to_string(),
        visible: true,
        locked: false,
        order: 0,
        items: Vec::new(),
    }]
}

pub(super) fn default_overlay_layouts() -> OverlayLayoutsFile {
    let now = now_ms();
    OverlayLayoutsFile {
        active_layout_id: "default".to_string(),
        stream_layout_id: "default".to_string(),
        layouts: vec![OverlayLayout {
            id: "default".to_string(),
            name: "Default".to_string(),
            width: 1920.0,
            height: 1080.0,
            layers: default_layers(),
            items: Vec::<OverlayItem>::new(),
            created_at_ms: now,
            updated_at_ms: now,
            template_source: None,
            thumbnail: None,
        }],
    }
}

pub(super) fn new_overlay_layout(
    id: String,
    name: String,
    width: Option<f64>,
    height: Option<f64>,
    now: u64,
) -> OverlayLayout {
    OverlayLayout {
        id,
        name: if name.trim().is_empty() {
            "Untitled Layout".to_string()
        } else {
            name
        },
        width: width.unwrap_or(1920.0).max(320.0),
        height: height.unwrap_or(1080.0).max(240.0),
        layers: default_layers(),
        items: Vec::new(),
        created_at_ms: now,
        updated_at_ms: now,
        template_source: None,
        thumbnail: None,
    }
}

pub(super) fn new_page_layout(
    id: String,
    name: String,
    open_target: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    now: u64,
) -> PageLayout {
    PageLayout {
        id,
        name: if name.trim().is_empty() {
            "Untitled Page".to_string()
        } else {
            name
        },
        favorite: false,
        width: width.unwrap_or(1440.0).max(320.0),
        height: height.unwrap_or(900.0).max(240.0),
        background: PageBackground::default(),
        settings: PageSettings {
            open_target: open_target.unwrap_or_else(|| "app".to_string()),
        },
        layers: default_page_layers(),
        created_at_ms: now,
        updated_at_ms: now,
        template_source: None,
        thumbnail: None,
    }
}
