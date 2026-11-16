#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod agents;
mod apps;
mod autostart;
mod brain;
mod controls;
mod debuglog;
mod display;
mod island;
mod localgit;
mod media;
mod osd;
mod phone;
mod pointer;
mod power;
mod redact;
mod report;
mod secrets;
mod settings;
mod shelf;
mod shortcuts;
mod sound;
mod sources;
mod system;
mod tasks;
mod today;
mod tray;
mod updater;
mod usage;
mod win;

use tauri::RunEvent;

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| settings::open_window(app)))
        .manage(island::Shared::default())
        .manage(usage::UsageHub::default())
        .manage(updater::Pending::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            island::island_update,
            island::island_typing,
            island::island_prewarm,
            island::island_pointer_inside,
            island::island_screens,
            settings::get_settings,
            settings::update_settings,
            settings::open_settings,
            settings::open_data_folder,
            updater::update_check,
            updater::update_state,
            updater::update_download,
            updater::update_install,
            updater::update_dismiss,
            apps::apps_pick,
            apps::apps_add,
            apps::apps_launch,
            actions::ops::web_open,
            actions::ops::web_search,
            actions::ops::claude_chat,
            actions::ops::app_open,
            actions::ops::app_close,
            actions::ops::files_copy,
            actions::ops::files_move,
            actions::ops::window_type,
            actions::ops::mail_draft,
            actions::ops::brain_pick_folder,
            apps::apps_icon,
            autostart::autostart_get,
            autostart::autostart_set,
            shortcuts::shortcuts_status,
            shortcuts::shortcuts_arm,
            shortcuts::shortcuts_pause,
            sound::sound_play,
            sound::sound_preview,
            sound::sound_quiet,
            sound::sound_upload,
            system::open_task_manager,
            power::power_status,
            power::power_present,
            report::report_info,
            report::report_debug,
            report::report_mark,
            system::system_watch,
            today::today_snapshot,
            today::today_apply,
            today::modes_pick_app,
            usage::usage_summary,
            shelf::shelf_snapshot,
            shelf::shelf_remove,
            shelf::shelf_reveal,
            shelf::shelf_drag_out,
            shelf::shelf_target,
            shelf::shelf_send,
            media::music_snapshot,
            media::music_control,
            media::music_seek,
            controls::controls_read,
            controls::controls_radio,
            controls::controls_volume,
            controls::controls_dark,
            controls::controls_outputs,
            controls::controls_output,
            controls::controls_open,
            agents::agents_snapshot,
            agents::agents_decide,
            agents::agents_demo,
            agents::agents_answer,
            agents::agents_pass,
            agents::agents_step,
            agents::agents_request_diff,
            agents::agents_focus,
            agents::agents_note,
            agents::agents_task,
            agents::agents_note_remove,
            agents::agents_open,
            brain::brain_status,
            brain::brain_check_login,
            brain::brain_ask,
            brain::brain_stop,
            phone::phone_status,
            phone::phone_pair_start,
            phone::phone_pair_confirm,
            phone::phone_pair_cancel,
            phone::phone_revoke,
            phone::phone_revoke_all,
            phone::phone_retry,
            agents::agents_editor_present,
            usage::usage_session,
            agents::setup::connections_inspect,
            agents::setup::connections_prepare,
            agents::setup::connections_apply,
            agents::statusline::statusline_inspect,
            agents::statusline::statusline_prepare,
            agents::statusline::plan_limits,
            actions::actions_list,
            actions::actions_pending,
            actions::actions_request,
            actions::actions_decide,
            actions::access::access_get,
            actions::access::access_set,
            actions::audit_recent,
            actions::audit_clear,
            tasks::list_recent_projects,
            tasks::list_closed_sessions,
            tasks::pick_folder,
            tasks::start_task,
            tasks::resume_session,
            tasks::task_stop,
            sources::sources_snapshot,
            sources::source_action,
            sources::source_preview,
            sources::source_refresh,
            sources::source_connect,
            sources::source_disconnect,
            sources::source_open_link,
            localgit::git_overview,
            localgit::git_commit,
            localgit::git_push,
            localgit::git_discard,
            localgit::git_fetch,
            localgit::git_pull,
            localgit::git_open_folder,
            localgit::git_pick_folder,
            localgit::git_desktop_repos,
        ])
        .setup(|app| {
            let handle = app.handle();
            let current = settings::init(handle)?;
            agents::init(handle, current.modules.agents);
            tasks::init(handle);
            phone::init(handle, current.modules.phone);
            brain::init(handle, current.modules.brain);
            media::init(handle, current.modules.music);
            system::init(handle);
            today::init(handle)?;
            shelf::init(handle)?;
            island::init(handle)?;
            actions::init(handle);
            sources::init(handle);
            shortcuts::init(handle, &current.active_shortcuts());
            osd::configure(handle, current.island.volume_osd);
            tray::init(handle, current.island.sound)?;
            updater::init(handle);
            sound::play(handle, sound::Cue::Hello);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Zodyvel başlatılamadı");

    app.run(|_app, event| {
        // Ayarlar penceresi kapanınca uygulama kapanmasın; çıkış yalnız tepsiden.
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
