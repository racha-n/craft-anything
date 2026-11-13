use super::*;
#[derive(Default, Serialize, Deserialize)]
pub(super) struct DeletionForm {
    open: bool,
}
impl DeletionForm {
    pub(super) fn show(&mut self, ui: &mut egui::Ui, task: &Task, snapshot: &Snapshot, pending: bool) -> Option<Value> {
        ui.toggle_value(&mut self.open, "Delete task");
        if !self.open {
            return None;
        }
        let mut request = None;
        ui.indent("task_deletion", |ui| {
            ui.label(format!("Selected: {}", task.label()));
            ui.small("Remove this task from the draft. Undo restores it; the source file stays unchanged.");
            if task.deletion.is_none() {
                ui.small("Deletion is unavailable for this saved task layout.");
                if !snapshot.deletion_reason.is_empty() {
                    ui.collapsing("Deletion details", |ui| {
                        ui.small(&snapshot.deletion_reason);
                    });
                }
            }
            if pending {
                ui.small("Apply pending edits or restore their values before deleting a task.");
            }
            let capacity = snapshot.order_edit_count < snapshot.order_edit_limit;
            if !capacity {
                ui.small("Export this candidate before starting another draft for more structural changes.");
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(task.deletion.is_some() && capacity && !pending, egui::Button::new("Delete from draft")).clicked()
                    && let Some(deletion) = &task.deletion
                {
                    request = Some(json!({"deletion":deletion}));
                }
                if ui.button("Cancel deletion").clicked() {
                    self.open = false;
                }
            });
            ui.small("The request is checked before this draft changes.");
        });
        request
    }
}
