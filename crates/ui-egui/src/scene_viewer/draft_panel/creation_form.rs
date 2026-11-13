use super::*;

#[derive(Default, Serialize, Deserialize)]
pub(super) struct CreationForm {
    name: String,
    display_id: String,
}
impl CreationForm {
    pub(super) fn new(display_id: &str) -> Self {
        Self { name: "New Work".into(), display_id: display_id.into() }
    }
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && !self.display_id.trim().is_empty()
            && !self.name.contains('\0')
            && !self.display_id.contains('\0')
            && self.name.len() <= 65_536
            && self.name.encode_utf16().count() <= 16_384
            && self.display_id.len() <= 1024
            && self.display_id.encode_utf16().count() <= 256
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui, task: &Task, snapshot: &Snapshot, pending: bool) -> Option<Value> {
        let mut request = None;
        ui.collapsing("Create task", |ui| {
            let Some(anchor) = &task.creation else {
                ui.small(if task.creation_reason.is_empty() { "Refresh this draft to inspect task creation." } else { &task.creation_reason });
                return;
            };
            ui.small("Create an unassigned one-day task above this row, using its start date and calendar.");
            ui.label(format!("Above: {}", task.label()));
            ui.horizontal(|ui| {
                let label = ui.label("New task name");
                ui.add(egui::TextEdit::singleline(&mut self.name).char_limit(16_384).desired_width(ui.available_width().clamp(180., 600.)))
                    .labelled_by(label.id);
            });
            ui.horizontal(|ui| {
                let label = ui.label("New task ID");
                ui.add(egui::TextEdit::singleline(&mut self.display_id).char_limit(256).desired_width(180.)).labelled_by(label.id);
            });
            let capacity = snapshot.order_edit_count < snapshot.order_edit_limit;
            if pending {
                ui.small("Apply pending edits or restore their values before creating a task.");
            }
            if !capacity {
                ui.small("Export this candidate before starting another draft for more structural changes.");
            }
            if !self.valid() {
                ui.small("Use a nonempty name and an ID of at most 256 UTF-16 units.");
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(capacity && !pending && self.valid(), egui::Button::new("Create one-day task above")).clicked() {
                    request = Some(json!({"anchor_task_guid":anchor.anchor_task_guid,"expected_parent_guid":anchor.expected_parent_guid,
                        "expected_sibling_index":anchor.expected_sibling_index,"name":self.name,"display_id":self.display_id}));
                }
                if ui.button("Cancel task creation").clicked() {
                    *self = Self::new(&snapshot.creation_suggested_display_id);
                }
            });
        });
        request
    }
}
