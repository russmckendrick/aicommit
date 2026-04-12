/// Interpolate between two hex colours by `t` (0.0 = start, 1.0 = end).
pub fn lerp_colour(start: &str, end: &str, t: f64) -> String {
    let t = t.clamp(0.0, 1.0);
    let (sr, sg, sb) = hex_to_rgb(start);
    let (er, eg, eb) = hex_to_rgb(end);
    let r = (sr as f64 + (er as f64 - sr as f64) * t).round() as u8;
    let g = (sg as f64 + (eg as f64 - sg as f64) * t).round() as u8;
    let b = (sb as f64 + (eb as f64 - sb as f64) * t).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Multi-stop gradient: given a set of hex colours and a `t` in [0, 1],
/// interpolate across the stops.
pub fn gradient(stops: &[&str], t: f64) -> String {
    if stops.is_empty() {
        return "#888888".to_owned();
    }
    if stops.len() == 1 {
        return stops[0].to_owned();
    }
    let t = t.clamp(0.0, 1.0);
    let segments = stops.len() - 1;
    let scaled = t * segments as f64;
    let idx = (scaled.floor() as usize).min(segments - 1);
    let local_t = scaled - idx as f64;
    lerp_colour(stops[idx], stops[idx + 1], local_t)
}

/// Heat scale: cold (low) = blue-grey, warm = orange, hot = red.
pub fn heat_colour(t: f64) -> String {
    gradient(&["#e8eaf6", "#42a5f5", "#ffb74d", "#ef5350"], t)
}

/// Activity scale: empty = light grey, active = green.
pub fn activity_colour(t: f64) -> String {
    gradient(&["#ebedf0", "#9be9a8", "#40c463", "#30a14e", "#216e39"], t)
}

/// Treemap directory palette: given a directory index, return a hue-shifted colour.
pub fn directory_colour(index: usize) -> String {
    const PALETTE: &[&str] = &[
        "#4fc3f7", "#81c784", "#ffb74d", "#e57373", "#ba68c8", "#4dd0e1", "#aed581", "#ffd54f",
        "#ff8a65", "#9575cd", "#26c6da", "#dce775", "#ffca28", "#ff7043", "#7e57c2",
    ];
    PALETTE[index % PALETTE.len()].to_owned()
}

fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    (r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_colour_midpoint() {
        let mid = lerp_colour("#000000", "#ffffff", 0.5);
        assert_eq!(mid, "#808080");
    }

    #[test]
    fn gradient_endpoints() {
        assert_eq!(gradient(&["#000000", "#ffffff"], 0.0), "#000000");
        assert_eq!(gradient(&["#000000", "#ffffff"], 1.0), "#ffffff");
    }

    #[test]
    fn directory_colour_wraps() {
        let a = directory_colour(0);
        let b = directory_colour(15);
        assert_eq!(a, b);
    }
}
