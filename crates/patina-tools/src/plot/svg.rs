use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct SvgLineSeries {
    pub label: String,
    pub color: String,
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SvgHistogramBin {
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SvgChartSpec {
    pub title: String,
    pub subtitle: Option<String>,
    pub x_label: String,
    pub y_label: String,
}

pub fn write_line_chart_svg(
    path: &Path,
    spec: &SvgChartSpec,
    series: &[SvgLineSeries],
) -> Result<(), std::io::Error> {
    std::fs::write(path, render_line_chart_svg(spec, series))
}

pub fn write_histogram_chart_svg(
    path: &Path,
    spec: &SvgChartSpec,
    bins: &[SvgHistogramBin],
    bar_color: &str,
) -> Result<(), std::io::Error> {
    std::fs::write(path, render_histogram_chart_svg(spec, bins, bar_color))
}

fn render_line_chart_svg(spec: &SvgChartSpec, series: &[SvgLineSeries]) -> String {
    let width = 960.0;
    let height = 540.0;
    let margin_left = 88.0;
    let margin_right = 28.0;
    let margin_top = 72.0;
    let margin_bottom = 64.0;
    let plot_width = width - margin_left - margin_right;
    let plot_height = height - margin_top - margin_bottom;

    let points = series
        .iter()
        .flat_map(|series| series.points.iter().copied())
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect::<Vec<_>>();

    let mut svg = svg_header(width, height, &spec.title);
    render_background(&mut svg, width, height);
    render_title_block(&mut svg, spec, 36.0, 38.0);

    if points.is_empty() {
        render_empty_state(&mut svg, width, height, "No finite data points available");
        svg.push_str("</svg>\n");
        return svg;
    }

    let (mut x_min, mut x_max) = min_max(points.iter().map(|(x, _)| *x));
    let (mut y_min, mut y_max) = min_max(points.iter().map(|(_, y)| *y));
    expand_if_flat(&mut x_min, &mut x_max);
    expand_if_flat(&mut y_min, &mut y_max);

    render_plot_frame(
        &mut svg,
        margin_left,
        margin_top,
        plot_width,
        plot_height,
        &tick_values(y_min, y_max, 6),
        &tick_values(x_min, x_max, 6),
        x_min,
        x_max,
        y_min,
        y_max,
        &spec.x_label,
        &spec.y_label,
    );

    for series in series {
        let finite_points = series
            .points
            .iter()
            .copied()
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .collect::<Vec<_>>();
        if finite_points.is_empty() {
            continue;
        }

        let polyline = finite_points
            .iter()
            .map(|(x, y)| {
                let px = map_range(*x, x_min, x_max, margin_left, margin_left + plot_width);
                let py = map_range(*y, y_min, y_max, margin_top + plot_height, margin_top);
                format!("{px:.2},{py:.2}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        let _ = writeln!(
            svg,
            "<polyline fill=\"none\" stroke=\"{}\" stroke-width=\"2.5\" points=\"{}\" />",
            escape_xml(&series.color),
            polyline
        );

        for (x, y) in &finite_points {
            let px = map_range(*x, x_min, x_max, margin_left, margin_left + plot_width);
            let py = map_range(*y, y_min, y_max, margin_top + plot_height, margin_top);
            let _ = writeln!(
                svg,
                "<circle cx=\"{px:.2}\" cy=\"{py:.2}\" r=\"3.2\" fill=\"{}\" />",
                escape_xml(&series.color)
            );
        }
    }

    render_legend(
        &mut svg,
        series
            .iter()
            .filter(|series| !series.points.is_empty())
            .map(|series| (series.label.as_str(), series.color.as_str()))
            .collect::<Vec<_>>()
            .as_slice(),
        width - margin_right - 180.0,
        margin_top - 8.0,
    );

    svg.push_str("</svg>\n");
    svg
}

fn render_histogram_chart_svg(
    spec: &SvgChartSpec,
    bins: &[SvgHistogramBin],
    bar_color: &str,
) -> String {
    let width = 960.0;
    let height = 540.0;
    let margin_left = 88.0;
    let margin_right = 28.0;
    let margin_top = 72.0;
    let margin_bottom = 64.0;
    let plot_width = width - margin_left - margin_right;
    let plot_height = height - margin_top - margin_bottom;

    let mut svg = svg_header(width, height, &spec.title);
    render_background(&mut svg, width, height);
    render_title_block(&mut svg, spec, 36.0, 38.0);

    let finite_bins = bins
        .iter()
        .filter(|bin| {
            bin.lower_bound.is_finite()
                && bin.upper_bound.is_finite()
                && bin.upper_bound >= bin.lower_bound
        })
        .collect::<Vec<_>>();
    if finite_bins.is_empty() {
        render_empty_state(&mut svg, width, height, "No histogram bins available");
        svg.push_str("</svg>\n");
        return svg;
    }

    let mut x_min = finite_bins
        .iter()
        .map(|bin| bin.lower_bound)
        .fold(f64::INFINITY, f64::min);
    let mut x_max = finite_bins
        .iter()
        .map(|bin| bin.upper_bound)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut y_min = 0.0;
    let mut y_max = finite_bins
        .iter()
        .map(|bin| bin.count as f64)
        .fold(0.0, f64::max);
    expand_if_flat(&mut x_min, &mut x_max);
    if y_max <= 0.0 {
        y_max = 1.0;
    }
    expand_if_flat(&mut y_min, &mut y_max);

    render_plot_frame(
        &mut svg,
        margin_left,
        margin_top,
        plot_width,
        plot_height,
        &tick_values(y_min, y_max, 6),
        &tick_values(x_min, x_max, 6),
        x_min,
        x_max,
        y_min,
        y_max,
        &spec.x_label,
        &spec.y_label,
    );

    for bin in finite_bins {
        let left = map_range(
            bin.lower_bound,
            x_min,
            x_max,
            margin_left,
            margin_left + plot_width,
        );
        let right = map_range(
            bin.upper_bound,
            x_min,
            x_max,
            margin_left,
            margin_left + plot_width,
        );
        let top = map_range(
            bin.count as f64,
            y_min,
            y_max,
            margin_top + plot_height,
            margin_top,
        );
        let bar_width = (right - left).max(1.0);
        let bar_height = margin_top + plot_height - top;
        let _ = writeln!(
            svg,
            "<rect x=\"{left:.2}\" y=\"{top:.2}\" width=\"{bar_width:.2}\" height=\"{bar_height:.2}\" fill=\"{}\" opacity=\"0.82\" />",
            escape_xml(bar_color)
        );
    }

    svg.push_str("</svg>\n");
    svg
}

fn svg_header(width: f64, height: f64, title: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\" role=\"img\" aria-label=\"{}\">\n",
        escape_xml(title)
    )
}

fn render_background(svg: &mut String, width: f64, height: f64) {
    let _ = writeln!(
        svg,
        "<rect x=\"0\" y=\"0\" width=\"{width:.0}\" height=\"{height:.0}\" fill=\"#fbfbfd\" />"
    );
}

fn render_title_block(svg: &mut String, spec: &SvgChartSpec, x: f64, y: f64) {
    let _ = writeln!(
        svg,
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-size=\"24\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#111827\">{}</text>",
        escape_xml(&spec.title)
    );
    if let Some(subtitle) = &spec.subtitle {
        let _ = writeln!(
            svg,
            "<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"13\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#4b5563\">{}</text>",
            y + 20.0,
            escape_xml(subtitle)
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_plot_frame(
    svg: &mut String,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    y_ticks: &[f64],
    x_ticks: &[f64],
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    x_label: &str,
    y_label: &str,
) {
    let _ = writeln!(
        svg,
        "<rect x=\"{left:.2}\" y=\"{top:.2}\" width=\"{width:.2}\" height=\"{height:.2}\" fill=\"#ffffff\" stroke=\"#d1d5db\" stroke-width=\"1\" />"
    );

    for tick in y_ticks {
        let y = map_range(*tick, y_min, y_max, top + height, top);
        let _ = writeln!(
            svg,
            "<line x1=\"{left:.2}\" y1=\"{y:.2}\" x2=\"{:.2}\" y2=\"{y:.2}\" stroke=\"#e5e7eb\" stroke-width=\"1\" />",
            left + width
        );
        let _ = writeln!(
            svg,
            "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"11\" text-anchor=\"end\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#4b5563\">{}</text>",
            left - 10.0,
            y + 4.0,
            format_tick(*tick)
        );
    }

    for tick in x_ticks {
        let x = map_range(*tick, x_min, x_max, left, left + width);
        let _ = writeln!(
            svg,
            "<line x1=\"{x:.2}\" y1=\"{top:.2}\" x2=\"{x:.2}\" y2=\"{:.2}\" stroke=\"#f1f5f9\" stroke-width=\"1\" />",
            top + height
        );
        let _ = writeln!(
            svg,
            "<text x=\"{x:.2}\" y=\"{:.2}\" font-size=\"11\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#4b5563\">{}</text>",
            top + height + 18.0,
            format_tick(*tick)
        );
    }

    let _ = writeln!(
        svg,
        "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"13\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#111827\">{}</text>",
        left + width / 2.0,
        top + height + 42.0,
        escape_xml(x_label)
    );
    let _ = writeln!(
        svg,
        "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"13\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#111827\" transform=\"rotate(-90 {:.2} {:.2})\">{}</text>",
        left - 54.0,
        top + height / 2.0,
        left - 54.0,
        top + height / 2.0,
        escape_xml(y_label)
    );
}

fn render_legend(svg: &mut String, items: &[(&str, &str)], x: f64, y: f64) {
    if items.is_empty() {
        return;
    }

    let height = 20.0 * items.len() as f64 + 16.0;
    let _ = writeln!(
        svg,
        "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"160\" height=\"{height:.2}\" rx=\"8\" fill=\"#ffffff\" stroke=\"#d1d5db\" />"
    );

    for (index, (label, color)) in items.iter().enumerate() {
        let y_offset = y + 22.0 + index as f64 * 20.0;
        let _ = writeln!(
            svg,
            "<line x1=\"{:.2}\" y1=\"{y_offset:.2}\" x2=\"{:.2}\" y2=\"{y_offset:.2}\" stroke=\"{}\" stroke-width=\"3\" />",
            x + 12.0,
            x + 34.0,
            escape_xml(color)
        );
        let _ = writeln!(
            svg,
            "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"12\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#111827\">{}</text>",
            x + 42.0,
            y_offset + 4.0,
            escape_xml(label)
        );
    }
}

fn render_empty_state(svg: &mut String, width: f64, height: f64, message: &str) {
    let _ = writeln!(
        svg,
        "<text x=\"{:.2}\" y=\"{:.2}\" font-size=\"18\" text-anchor=\"middle\" font-family=\"Helvetica, Arial, sans-serif\" fill=\"#6b7280\">{}</text>",
        width / 2.0,
        height / 2.0,
        escape_xml(message)
    );
}

fn min_max(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values {
        min = min.min(value);
        max = max.max(value);
    }
    (min, max)
}

fn expand_if_flat(min: &mut f64, max: &mut f64) {
    if !min.is_finite() || !max.is_finite() {
        *min = 0.0;
        *max = 1.0;
        return;
    }
    if (*max - *min).abs() <= f64::EPSILON {
        let padding = if *min == 0.0 { 1.0 } else { min.abs() * 0.05 };
        *min -= padding;
        *max += padding;
        return;
    }
    let padding = (*max - *min) * 0.05;
    *min -= padding;
    *max += padding;
}

fn map_range(value: f64, src_min: f64, src_max: f64, dst_min: f64, dst_max: f64) -> f64 {
    if (src_max - src_min).abs() <= f64::EPSILON {
        return (dst_min + dst_max) / 2.0;
    }
    dst_min + (value - src_min) * (dst_max - dst_min) / (src_max - src_min)
}

fn tick_values(min: f64, max: f64, count: usize) -> Vec<f64> {
    if count <= 1 {
        return vec![min, max];
    }
    let step = (max - min) / (count - 1) as f64;
    (0..count).map(|index| min + step * index as f64).collect()
}

fn format_tick(value: f64) -> String {
    if value.abs() >= 100.0 || (value.abs() > 0.0 && value.abs() < 0.01) {
        format!("{value:.2e}")
    } else if (value - value.round()).abs() < 1.0e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.3}")
    }
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
