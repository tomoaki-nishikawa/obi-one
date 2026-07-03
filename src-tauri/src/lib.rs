mod commands;
mod core;
mod output;
mod processor;
mod settings;

use commands::{
    choose_input_pdfs, choose_output_dir, choose_template_pdf, load_settings, open_path,
    process_one, reset_template, save_settings,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            choose_input_pdfs,
            choose_output_dir,
            choose_template_pdf,
            reset_template,
            process_one,
            open_path,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run obi-one");
}
