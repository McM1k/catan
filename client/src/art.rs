//! Hand-drawn SVG illustrations for the board tiles.
//!
//! Every piece of art is drawn around the origin (the tile centre) inside a
//! hexagon of circumradius 1, leaving the middle free for the number token.
//! They are original drawings, kept as markup strings and injected with
//! `inner_html`, so no image files have to be loaded or shipped.

use engine::Terrain;
use leptos::prelude::*;

/// Gradients used as tile backgrounds plus the hexagon clip path.
pub const DEFS: &str = r##"
<clipPath id="hexclip"><polygon points="0,-1 0.866,-0.5 0.866,0.5 0,1 -0.866,0.5 -0.866,-0.5"/></clipPath>
<linearGradient id="g-forest" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4a9b6d"/><stop offset="1" stop-color="#245a40"/></linearGradient>
<linearGradient id="g-hills" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#e08a62"/><stop offset="1" stop-color="#a8452d"/></linearGradient>
<linearGradient id="g-pasture" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#b7e8c4"/><stop offset="1" stop-color="#6fbf8a"/></linearGradient>
<linearGradient id="g-fields" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f7dc92"/><stop offset="1" stop-color="#d9a93f"/></linearGradient>
<linearGradient id="g-mountains" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#b9c4d4"/><stop offset="1" stop-color="#7d8aa0"/></linearGradient>
<linearGradient id="g-desert" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f3e2ae"/><stop offset="1" stop-color="#dcc183"/></linearGradient>
"##;

pub fn gradient_id(t: Terrain) -> &'static str {
    match t {
        Terrain::Forest => "g-forest",
        Terrain::Hills => "g-hills",
        Terrain::Pasture => "g-pasture",
        Terrain::Fields => "g-fields",
        Terrain::Mountains => "g-mountains",
        Terrain::Desert => "g-desert",
    }
}

/// The illustration for a tile centred on (`cx`, `cy`). `seed` (the tile's
/// index) mirrors every other tile so neighbours of one terrain don't look
/// like copies of each other.
pub fn tile_art(t: Terrain, cx: f64, cy: f64, seed: usize) -> impl IntoView {
    let flip = if seed.is_multiple_of(2) { 1 } else { -1 };
    view! { <g transform=format!("translate({cx:.3} {cy:.3}) scale({flip} 1)") inner_html=art_markup(t) /> }
}

/// The illustration for a terrain, clipped to the hexagon.
fn art_markup(t: Terrain) -> String {
    let body = match t {
        Terrain::Forest => forest(),
        Terrain::Hills => hills(),
        Terrain::Pasture => pasture(),
        Terrain::Fields => fields(),
        Terrain::Mountains => mountains(),
        Terrain::Desert => desert(),
    };
    format!(r#"<g clip-path="url(#hexclip)">{body}</g>"#)
}

fn at(x: f64, y: f64, s: f64, inner: &str) -> String {
    format!(r#"<g transform="translate({x} {y}) scale({s})">{inner}</g>"#)
}

fn forest() -> String {
    const TREE: &str = r##"
<ellipse cx="0" cy="0.2" rx="0.2" ry="0.04" fill="#0d3b25" opacity="0.35"/>
<rect x="-0.025" y="0.11" width="0.05" height="0.1" fill="#5b3a1e"/>
<polygon points="0,-0.1 0.24,0.14 -0.24,0.14" fill="#348a58" stroke="#13422a" stroke-width="0.012"/>
<polygon points="0,-0.24 0.2,0.04 -0.2,0.04" fill="#2a7048" stroke="#13422a" stroke-width="0.012"/>
<polygon points="0,-0.37 0.15,-0.08 -0.15,-0.08" fill="#1f5a3a" stroke="#13422a" stroke-width="0.012"/>"##;
    [
        (-0.60, -0.30, 0.95),
        (0.58, -0.36, 0.9),
        (-0.68, 0.14, 0.85),
        (0.64, 0.12, 1.0),
        (-0.30, 0.60, 0.95),
        (0.34, 0.58, 0.85),
        (0.0, -0.62, 0.8),
        (-0.40, -0.62, 0.6),
        (0.42, 0.88, 0.6),
    ]
    .iter()
    .map(|&(x, y, s)| at(x, y, s, TREE))
    .collect()
}

fn hills() -> String {
    const BRICKS: &str = r##"
<g fill="#e8825c" stroke="#6e2815" stroke-width="0.014">
<rect x="-0.22" y="0" width="0.21" height="0.09" rx="0.012"/><rect x="0.01" y="0" width="0.21" height="0.09" rx="0.012"/>
<rect x="-0.11" y="-0.09" width="0.21" height="0.09" rx="0.012"/>
<rect x="-0.22" y="0.09" width="0.21" height="0.09" rx="0.012"/><rect x="0.01" y="0.09" width="0.21" height="0.09" rx="0.012"/>
</g>"##;
    format!(
        r##"<path d="M-0.95,0.55 Q-0.6,0.0 -0.15,0.45 Q0.3,-0.1 0.95,0.4 L0.95,1 L-0.95,1 Z" fill="#8e3a24"/>
<path d="M-0.95,0.75 Q-0.45,0.35 0.1,0.7 Q0.55,0.4 0.95,0.7 L0.95,1 L-0.95,1 Z" fill="#b84e33"/>
<path d="M-0.9,-0.25 Q-0.5,-0.55 -0.1,-0.3" stroke="#f0b394" stroke-width="0.04" fill="none" opacity="0.5"/>
{}{}{}"##,
        at(-0.56, 0.22, 0.8, BRICKS),
        at(0.52, 0.16, 0.7, BRICKS),
        at(0.0, 0.7, 0.55, BRICKS),
    )
}

fn pasture() -> String {
    const SHEEP: &str = r##"
<ellipse cx="0" cy="0.18" rx="0.2" ry="0.035" fill="#2f6e46" opacity="0.3"/>
<rect x="-0.11" y="0.07" width="0.03" height="0.1" fill="#3b3b3b"/><rect x="0.08" y="0.07" width="0.03" height="0.1" fill="#3b3b3b"/>
<ellipse cx="0" cy="0" rx="0.17" ry="0.11" fill="#fbfbf5" stroke="#8a8a80" stroke-width="0.012"/>
<circle cx="-0.1" cy="-0.05" r="0.07" fill="#fbfbf5"/><circle cx="0.02" cy="-0.08" r="0.075" fill="#fbfbf5"/><circle cx="0.1" cy="-0.04" r="0.06" fill="#fbfbf5"/>
<ellipse cx="0.2" cy="-0.02" rx="0.065" ry="0.05" fill="#3b3b3b"/>
<circle cx="0.22" cy="-0.04" r="0.01" fill="#fff"/>"##;
    const TUFT: &str = r##"<path d="M0,0 L-0.04,-0.1 M0,0 L0,-0.13 M0,0 L0.04,-0.1" stroke="#3d8a58" stroke-width="0.025" stroke-linecap="round" fill="none"/>"##;
    let mut s = String::from(
        r##"<path d="M-0.95,0.35 Q-0.45,0.15 0.05,0.35 T0.95,0.3" stroke="#8fd3a4" stroke-width="0.07" fill="none" opacity="0.7"/>
<path d="M-0.95,-0.35 Q-0.4,-0.55 0.1,-0.35 T0.95,-0.4" stroke="#8fd3a4" stroke-width="0.06" fill="none" opacity="0.6"/>"##,
    );
    for &(x, y, k) in &[
        (-0.52, -0.34, 1.0),
        (0.5, -0.28, -0.85),
        (-0.1, 0.62, 0.95),
        (0.62, 0.38, -0.6),
    ] {
        let sx: f64 = k;
        s += &at(x, y, sx.abs(), &if sx < 0.0 {
            format!(r#"<g transform="scale(-1 1)">{SHEEP}</g>"#)
        } else {
            SHEEP.to_string()
        });
    }
    for &(x, y) in &[(-0.75, 0.05), (0.72, -0.05), (-0.42, 0.3), (0.4, 0.82), (0.0, -0.78), (-0.7, 0.62)] {
        s += &at(x, y, 0.9, TUFT);
    }
    s
}

fn fields() -> String {
    const STALK: &str = r##"
<line x1="0" y1="0.2" x2="0" y2="-0.12" stroke="#8a6a1f" stroke-width="0.022" stroke-linecap="round"/>
<g fill="#f6cf62" stroke="#a8801e" stroke-width="0.008">
<ellipse cx="-0.035" cy="-0.04" rx="0.026" ry="0.05" transform="rotate(-28 -0.035 -0.04)"/>
<ellipse cx="0.035" cy="-0.04" rx="0.026" ry="0.05" transform="rotate(28 0.035 -0.04)"/>
<ellipse cx="-0.035" cy="-0.1" rx="0.026" ry="0.05" transform="rotate(-28 -0.035 -0.1)"/>
<ellipse cx="0.035" cy="-0.1" rx="0.026" ry="0.05" transform="rotate(28 0.035 -0.1)"/>
<ellipse cx="0" cy="-0.19" rx="0.026" ry="0.055"/>
</g>"##;
    let mut s = String::from(r##"<g stroke="#c99a38" stroke-width="0.04" opacity="0.55" stroke-linecap="round">"##);
    for i in 0..8 {
        let y = -0.9 + 0.27 * i as f64;
        s += &format!(r#"<line x1="-1" y1="{:.3}" x2="1" y2="{:.3}"/>"#, y + 0.25, y - 0.25);
    }
    s += "</g>";
    for &(x, y, k) in &[
        (-0.62, -0.30, 1.0),
        (-0.52, -0.26, 0.9),
        (0.58, -0.36, 1.0),
        (0.48, -0.32, 0.85),
        (-0.70, 0.18, 0.95),
        (0.66, 0.22, 1.0),
        (0.56, 0.26, 0.85),
        (-0.30, 0.64, 0.95),
        (-0.20, 0.68, 0.8),
        (0.30, 0.66, 1.0),
        (0.0, -0.66, 0.85),
    ] {
        s += &at(x, y, k, STALK);
    }
    s
}

fn mountains() -> String {
    const NUGGET: &str = r##"<polygon points="-0.06,0 0.04,-0.05 0.11,0.02 0.05,0.08 -0.03,0.07" fill="#c5ceda" stroke="#4a5568" stroke-width="0.012"/><polygon points="0.04,-0.05 0.11,0.02 0.05,0.0" fill="#fff" opacity="0.6"/>"##;
    format!(
        r##"<polygon points="-0.95,0.7 -0.42,-0.34 0.12,0.7" fill="#6b7689" stroke="#4a5568" stroke-width="0.016"/>
<polygon points="-0.42,-0.34 0.12,0.7 -0.2,0.7" fill="#4f596b" opacity="0.55"/>
<polygon points="-0.05,0.7 0.44,-0.18 0.98,0.7" fill="#8995a9" stroke="#4a5568" stroke-width="0.016"/>
<polygon points="0.44,-0.18 0.98,0.7 0.62,0.7" fill="#66728a" opacity="0.55"/>
<polygon points="-0.42,-0.34 -0.55,-0.1 -0.47,-0.14 -0.4,-0.06 -0.33,-0.15 -0.27,-0.1" fill="#f4f7fa"/>
<polygon points="0.44,-0.18 0.33,0.04 0.4,-0.01 0.45,0.07 0.52,-0.01 0.57,0.04" fill="#f4f7fa"/>
{}{}{}"##,
        at(-0.5, 0.78, 0.9, NUGGET),
        at(0.5, 0.82, 0.75, NUGGET),
        at(0.0, -0.7, 0.6, NUGGET),
    )
}

fn desert() -> String {
    const CACTUS: &str = r##"
<g fill="#4f8f4a" stroke="#2f5e2c" stroke-width="0.012">
<rect x="-0.045" y="-0.22" width="0.09" height="0.4" rx="0.045"/>
<path d="M-0.045,0.02 h-0.09 a0.04,0.04 0 0 1 -0.04,-0.04 v-0.1 a0.035,0.035 0 0 1 0.07,0 v0.07 h0.06 z"/>
<path d="M0.045,-0.04 h0.08 a0.04,0.04 0 0 0 0.04,-0.04 v-0.08 a0.035,0.035 0 0 0 -0.07,0 v0.05 h-0.05 z"/>
</g>"##;
    format!(
        r##"<circle cx="0.55" cy="-0.5" r="0.15" fill="#f8c94a" opacity="0.9"/>
<circle cx="0.55" cy="-0.5" r="0.22" fill="#f8c94a" opacity="0.25"/>
<path d="M-0.95,0.3 Q-0.45,-0.05 0.05,0.3 T0.95,0.25 L0.95,1 L-0.95,1 Z" fill="#e8d196"/>
<path d="M-0.95,0.62 Q-0.3,0.35 0.25,0.62 T0.95,0.58 L0.95,1 L-0.95,1 Z" fill="#d4b56d"/>
<path d="M-0.6,0.8 q0.15,-0.06 0.3,0 M0.2,0.86 q0.12,-0.05 0.25,0" stroke="#bf9f58" stroke-width="0.025" fill="none" stroke-linecap="round"/>
{}"##,
        at(-0.58, 0.18, 0.95, CACTUS),
    )
}
