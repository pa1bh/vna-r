//! A Smith chart drawn with egui's painter.

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use num_complex::Complex64;

pub struct SmithChart<'a> {
    pub gamma: &'a [Complex64],
    pub marker: Option<Complex64>,
    pub trace_color: Color32,
}

pub struct SmithResponse {
    pub response: Response,
    pub center: Pos2,
    pub radius: f32,
}

impl SmithResponse {
    /// Screen position of a reflection coefficient on this chart.
    pub fn pos_of(&self, g: Complex64) -> Pos2 {
        self.center + Vec2::new(g.re as f32, -g.im as f32) * self.radius
    }
}

impl<'a> SmithChart<'a> {
    pub fn show(self, ui: &mut Ui, size: Vec2) -> SmithResponse {
        let (response, painter) = ui.allocate_painter(size, Sense::click());
        let rect: Rect = response.rect;
        let radius = (rect.width().min(rect.height()) / 2.0 - 14.0).max(10.0);
        let center = rect.center();
        let to_screen = |g: Complex64| center + Vec2::new(g.re as f32, -g.im as f32) * radius;

        let grid = Stroke::new(1.0, Color32::from_gray(90));
        let grid_light = Stroke::new(0.7, Color32::from_gray(65));
        let text_color = Color32::from_gray(150);

        painter.rect_filled(rect, 4.0, Color32::from_gray(22));
        painter.circle_stroke(center, radius, grid);
        painter.line_segment([to_screen(Complex64::new(-1.0, 0.0)), to_screen(Complex64::new(1.0, 0.0))], grid);

        // Constant-resistance circles: centre (r/(1+r), 0), radius 1/(1+r).
        for r in [0.2, 0.5, 1.0, 2.0, 5.0] {
            let c = to_screen(Complex64::new(r / (1.0 + r), 0.0));
            painter.circle_stroke(c, radius / (1.0 + r) as f32, if r == 1.0 { grid } else { grid_light });
            let label = to_screen(Complex64::new((r - 1.0) / (r + 1.0), 0.0));
            painter.text(label + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, format!("{r}"), FontId::proportional(10.0), text_color);
        }
        // Constant-reactance arcs: centre (1, 1/x), radius 1/|x|, clipped to the unit circle.
        for x in [0.2, 0.5, 1.0, 2.0, 5.0] {
            for sign in [1.0, -1.0] {
                let xs = x * sign;
                let cc = Complex64::new(1.0, 1.0 / xs);
                let rr = 1.0 / x;
                let mut run: Vec<Pos2> = Vec::new();
                for k in 0..=360 {
                    let a = k as f64 * std::f64::consts::TAU / 360.0;
                    let g = cc + Complex64::from_polar(rr, a);
                    if g.norm() <= 1.0005 {
                        run.push(to_screen(g));
                    } else if run.len() > 1 {
                        painter.add(egui::Shape::line(std::mem::take(&mut run), grid_light));
                    } else {
                        run.clear();
                    }
                }
                if run.len() > 1 {
                    painter.add(egui::Shape::line(run, grid_light));
                }
                // Label where the arc meets the unit circle: Γ = (jx−1)/(jx+1)
                let z = Complex64::new(0.0, xs);
                let g = (z - 1.0) / (z + 1.0);
                let p = to_screen(g * 1.08);
                painter.text(p, Align2::CENTER_CENTER, format!("{}{x}j", if sign > 0.0 { "+" } else { "−" }), FontId::proportional(10.0), text_color);
            }
        }

        // Trace
        if self.gamma.len() > 1 {
            let pts: Vec<Pos2> = self.gamma.iter().map(|g| to_screen(clamp(*g))).collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.8, self.trace_color)));
        }
        if let Some(m) = self.marker {
            let p = to_screen(clamp(m));
            painter.circle_filled(p, 4.5, Color32::WHITE);
            painter.circle_stroke(p, 6.0, Stroke::new(1.5, self.trace_color));
        }
        SmithResponse { response, center, radius }
    }
}

/// Keep wild uncalibrated points on the chart.
fn clamp(g: Complex64) -> Complex64 {
    let n = g.norm();
    if n > 1.15 {
        g / n * 1.15
    } else {
        g
    }
}
