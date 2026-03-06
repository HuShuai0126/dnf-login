use eframe::egui;

use super::DnfLoginApp;

// UI components
impl DnfLoginApp {
    pub(super) fn text_input<'a>(
        label: &str,
        value: &'a mut String,
        hint: &'a str,
    ) -> impl egui::Widget + 'a {
        let label = label.to_string();
        move |ui: &mut egui::Ui| {
            ui.label(
                egui::RichText::new(&label)
                    .size(13.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .font(egui::TextStyle::Body),
            )
        }
    }

    pub(super) fn password_input<'a>(
        label: &str,
        value: &'a mut String,
        hint: &'a str,
    ) -> impl egui::Widget + 'a {
        let label = label.to_string();
        move |ui: &mut egui::Ui| {
            ui.label(
                egui::RichText::new(&label)
                    .size(13.0)
                    .color(Self::c_text2()),
            );
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(value)
                    .password(true)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint)
                    .font(egui::TextStyle::Body),
            )
        }
    }

    pub(super) fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
        let h = 44.0;
        let w = ui.available_width();
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), sense);

        if ui.is_rect_visible(rect) {
            let fill = if !enabled {
                Self::c_border_dim()
            } else if response.is_pointer_button_down_on() {
                Self::c_accent_press()
            } else if response.hovered() {
                Self::c_accent_hover()
            } else {
                Self::c_accent()
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(8), fill);
            let text_col = if enabled {
                egui::Color32::WHITE
            } else {
                Self::c_text3()
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(14.5),
                text_col,
            );
        }
        response.clicked()
    }

    pub(super) fn primary_button_slim(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
        let h = 36.0;
        let w = ui.available_width();
        let sense = if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(w, h), sense);

        if ui.is_rect_visible(rect) {
            let fill = if !enabled {
                Self::c_border_dim()
            } else if response.is_pointer_button_down_on() {
                Self::c_accent_press()
            } else if response.hovered() {
                Self::c_accent_hover()
            } else {
                Self::c_accent()
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), fill);
            let text_col = if enabled {
                egui::Color32::WHITE
            } else {
                Self::c_text3()
            };
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(14.0),
                text_col,
            );
        }
        response.clicked()
    }

    pub(super) fn secondary_button(label: &str) -> egui::Button<'static> {
        egui::Button::new(
            egui::RichText::new(label.to_string())
                .size(13.5)
                .color(Self::c_text2()),
        )
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::new(1.0, Self::c_border()))
        .corner_radius(egui::CornerRadius::same(6))
        .min_size(egui::vec2(0.0, 36.0))
    }

    pub(super) fn warning_box(ui: &mut egui::Ui, text: &str) {
        egui::Frame::new()
            .fill(Self::c_warn_bg())
            .stroke(egui::Stroke::new(1.0, Self::c_warn_border()))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::vec2(12.0, 8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .size(13.0)
                            .color(Self::c_warn_text()),
                    );
                });
            });
    }

    pub(super) fn status_box(ui: &mut egui::Ui, text: &str, is_error: bool) {
        let (bg, border, color) = if is_error {
            (Self::c_error_bg(), Self::c_error(), Self::c_error())
        } else {
            (Self::c_success_bg(), Self::c_success(), Self::c_success())
        };
        egui::Frame::new()
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, border))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::vec2(14.0, 10.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(text).size(13.5).color(color));
                });
            });
    }
}
