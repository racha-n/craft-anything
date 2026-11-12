//! A thin chooser over the engine's typed report-template catalogue.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Preset {
    Legacy,
    #[default]
    All,
    Schedule,
    Costs,
    Custom,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    preset: Preset,
    template_path: String,
}
impl Choice {
    pub fn show(&mut self, ui: &mut egui::Ui, enabled: bool) -> bool {
        let labels = [
            (Preset::All, "All tables with dates"),
            (Preset::Schedule, "Schedule review"),
            (Preset::Costs, "Cost review"),
            (Preset::Custom, "Saved template JSON"),
            (Preset::Legacy, "Legacy raw CSV"),
        ];
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label("CSV template");
                let label = labels.iter().find(|(p, _)| *p == self.preset).map_or("Choose template", |(_, label)| *label);
                let response = egui::ComboBox::from_id_salt("report_template_choice")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (preset, label) in labels {
                            ui.selectable_value(&mut self.preset, preset, label);
                        }
                    })
                    .response;
                crate::widgets::named(response, "CSV template selection");
            });
            if self.preset == Preset::Custom {
                ui.horizontal(|ui| {
                    ui.label("Template JSON path");
                    crate::widgets::named(ui.text_edit_singleline(&mut self.template_path), "Template JSON path");
                });
            }
        });
        ui.small("Template selects CSV tables. snapshot.json retains the full raw report dataset. Reuse the exported template.json for the same selection.");
        self.preset != Preset::Custom || !self.template_path.trim().is_empty()
    }
    pub fn apply(&self, params: &mut Value) {
        let index = match self.preset {
            Preset::All => Some(0),
            Preset::Schedule => Some(1),
            Preset::Costs => Some(2),
            Preset::Legacy | Preset::Custom => None,
        };
        if let Some(index) = index
            && let Some(template) = craft_anything_engine::tools::scene_presentation_tools::report_template::presets().get(index)
        {
            params["template"] = json!(template);
        } else if self.preset == Preset::Custom {
            params["template_path"] = json!(self.template_path);
        }
    }
}
