#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};

slint::include_modules!();

const PLACEHOLDER_ROWS: usize = 8;

fn main() -> Result<(), slint::PlatformError> {
    // Placeholder rows until the real installed-app enumeration lands.
    let entries: Rc<VecModel<AppEntry>> = Rc::new(VecModel::from(
        (0..PLACEHOLDER_ROWS)
            .map(|_| AppEntry {
                name: "List item".into(),
                initial: "A".into(),
                date: "Date of Installation".into(),
                selected: true,
            })
            .collect::<Vec<AppEntry>>(),
    ));

    let ui = AppWindow::new()?;
    ui.set_entries(ModelRc::from(entries.clone()));

    {
        let entries = entries.clone();
        ui.on_toggle_entry(move |index| {
            if let Some(mut entry) = entries.row_data(index as usize) {
                entry.selected = !entry.selected;
                entries.set_row_data(index as usize, entry);
            }
        });
    }

    ui.on_uninstall_selected(move || {
        // Placeholder: the real uninstaller will hook in here.
        let selected: Vec<usize> = (0..entries.row_count())
            .filter(|index| entries.row_data(*index).is_some_and(|entry| entry.selected))
            .collect();

        for index in selected.into_iter().rev() {
            entries.remove(index);
        }
    });

    ui.run()
}
