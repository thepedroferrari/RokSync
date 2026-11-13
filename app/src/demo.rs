//! `roksync demo`: a sample home inventory that stands in for a bridge.

use lumen_hue::model::*;
use lumen_hue::{Inventory, Rgb};

fn rr(rid: &str, rtype: &str) -> ResourceRef {
    ResourceRef { rid: rid.into(), rtype: rtype.into() }
}

fn xy(hex: &str) -> LightColor {
    let (x, y) = Rgb::from_hex(hex).unwrap_or(Rgb::WHITE).to_xy();
    LightColor { xy: Xy { x, y } }
}

fn meta(name: &str) -> Metadata {
    Metadata { name: name.into(), archetype: String::new() }
}

struct L<'a> {
    id: &'a str,
    name: &'a str,
    product: &'a str,
    on: bool,
    bri: f64,
    color: Option<&'a str>,
    gradient: &'a [&'a str],
}

pub fn inventory() -> Inventory {
    let ls = [
        L { id: "l1", name: "Play bar left", product: "Hue Play", on: true, bri: 92.0, color: Some("#ff2bd6"), gradient: &[] },
        L { id: "l2", name: "Play bar right", product: "Hue Play", on: true, bri: 92.0, color: Some("#00e5ff"), gradient: &[] },
        L { id: "l3", name: "Desk gradient", product: "Hue Play gradient lightstrip", on: true, bri: 80.0, color: Some("#7b2ff7"), gradient: &["#ff2bd6", "#7b2ff7", "#2d7dff", "#00e5ff", "#00ffa3"] },
        L { id: "l4", name: "Shelf Go", product: "Hue Go", on: true, bri: 64.0, color: Some("#ff6a00"), gradient: &[] },
        L { id: "l5", name: "Ceiling", product: "Hue White and colour ambiance", on: false, bri: 100.0, color: Some("#ffd29a"), gradient: &[] },
        L { id: "l6", name: "TV gradient", product: "Hue Play gradient lightstrip", on: true, bri: 70.0, color: Some("#0090ff"), gradient: &["#00c2ff", "#0050ff", "#6a00ff", "#ff0080", "#ff6a00", "#ffb000", "#00ffa3"] },
        L { id: "l7", name: "Signe floor", product: "Hue Signe gradient", on: true, bri: 55.0, color: Some("#ff2e63"), gradient: &["#ff2e63", "#7b2ff7"] },
        L { id: "l8", name: "Sofa lamp", product: "Hue Iris", on: true, bri: 40.0, color: Some("#ffb000"), gradient: &[] },
        L { id: "l9", name: "Reading lamp", product: "Hue White ambiance", on: false, bri: 100.0, color: None, gradient: &[] },
        L { id: "l10", name: "Bedside left", product: "Hue Go", on: true, bri: 22.0, color: Some("#ff7a3d"), gradient: &[] },
        L { id: "l11", name: "Bedside right", product: "Hue Go", on: false, bri: 22.0, color: Some("#ff7a3d"), gradient: &[] },
        L { id: "l12", name: "Under-bed strip", product: "Hue Lightstrip Plus", on: true, bri: 30.0, color: Some("#3a00ff"), gradient: &[] },
        L { id: "l13", name: "Hallway", product: "Hue White ambiance", on: true, bri: 100.0, color: None, gradient: &[] },
    ];
    let mut inv = Inventory::default();
    for l in &ls {
        let dev = format!("d{}", &l.id[1..]);
        inv.lights.push(Light {
            id: l.id.into(),
            owner: rr(&dev, "device"),
            metadata: meta(l.name),
            on: OnState { on: l.on },
            dimming: Some(Dimming { brightness: l.bri }),
            color: l.color.map(xy),
            gradient: (!l.gradient.is_empty()).then(|| Gradient {
                points: l.gradient.iter().map(|h| GradientPoint { color: xy(h) }).collect(),
                points_capable: l.gradient.len() as u32,
            }),
            // every demo light can do white; white-only ones sit at 3000 K
            color_temperature: Some(LightCt { mirek: Some(333.0), mirek_valid: l.color.is_none() }),
            ..Default::default()
        });
        inv.devices.push(Device {
            id: dev.clone(),
            metadata: meta(l.name),
            product_data: ProductData { model_id: "LCX004".into(), product_name: l.product.into() },
            services: vec![rr(l.id, "light"), rr(&format!("es{}", &l.id[1..]), "entertainment")],
        });
        inv.ent_services.push(EntService { id: format!("es{}", &l.id[1..]), owner: rr(&dev, "device"), renderer: true, ..Default::default() });
    }
    let room = |id: &str, name: &str, lights: &[&str]| Group {
        id: id.into(),
        metadata: meta(name),
        children: lights.iter().map(|l| rr(&format!("d{}", &l[1..]), "device")).collect(),
        services: vec![rr(&format!("g{id}"), "grouped_light")],
    };
    inv.rooms = vec![
        room("r1", "Battlestation", &["l1", "l2", "l3", "l4", "l5"]),
        room("r2", "Living room", &["l6", "l7", "l8", "l9"]),
        room("r3", "Bedroom", &["l10", "l11", "l12"]),
        room("r4", "Hallway", &["l13"]),
    ];
    // the hallway light is off at the wall switch: the bridge can't reach it
    inv.connectivity = inv
        .lights
        .iter()
        .map(|l| ZigbeeConn {
            id: format!("zc{}", l.id),
            owner: l.owner.clone(),
            status: if l.id == "l13" { "connectivity_issue" } else { "connected" }.into(),
        })
        .collect();
    inv.zones = vec![Group {
        id: "z1".into(),
        metadata: meta("Desk + TV"),
        children: ["l1", "l2", "l3", "l6"].iter().map(|l| rr(l, "light")).collect(),
        services: vec![rr("gz1", "grouped_light")],
    }];
    for g in inv.rooms.iter().chain(inv.zones.iter()) {
        inv.grouped_lights.push(GroupedLight { id: format!("g{}", g.id), owner: rr(&g.id, "room"), on: OnState { on: true } });
    }
    let ch = |id: u8, x: f64, y: f64, z: f64, light: &str| Channel {
        channel_id: id,
        position: Position { x, y, z },
        members: vec![ChannelMember { service: rr(&format!("es{}", &light[1..]), "entertainment"), index: 0 }],
    };
    inv.ent_configs = vec![EntConfig {
        id: "e1".into(),
        metadata: meta("Battlestation"),
        configuration_type: "screen".into(),
        status: "inactive".into(),
        channels: vec![
            ch(0, -0.8, 0.8, 0.0, "l1"),
            ch(1, 0.8, 0.8, 0.0, "l2"),
            ch(2, -0.5, 0.95, -0.4, "l3"),
            ch(3, 0.0, 0.95, -0.4, "l3"),
            ch(4, 0.5, 0.95, -0.4, "l3"),
            ch(5, -0.95, -0.3, 0.4, "l4"),
            ch(6, 0.95, -0.5, 0.6, "l7"),
        ],
        ..Default::default()
    }];
    let scene = |id: &str, name: &str, room: &str, cols: &[&str]| Scene {
        id: id.into(),
        metadata: meta(name),
        group: rr(room, "room"),
        palette: Some(Palette { color: cols.iter().map(|h| PaletteColor { color: xy(h), dimming: None }).collect(), ..Default::default() }),
        ..Default::default()
    };
    inv.scenes = vec![
        scene("s1", "Night raid", "r1", &["#ff2bd6", "#7b2ff7", "#00e5ff"]),
        scene("s2", "Arctic ops", "r1", &["#e8f6ff", "#7fd3ff", "#0050ff"]),
        scene("s3", "Molten core", "r1", &["#ff2a00", "#ff8a00", "#ffd000"]),
        scene("s4", "Savanna sunset", "r2", &["#ff6a00", "#ff2e63", "#7b2ff7"]),
        scene("s5", "Deep forest", "r2", &["#00ff8a", "#00a36c", "#0a3d2a"]),
        scene("s6", "Nightlight", "r3", &["#ff7a3d", "#ff3d00"]),
    ];
    inv
}
