use anyhow::Result;
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{
    glib, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider,
    DropDown, Entry, Label, Orientation, ScrolledWindow, StringList, TextView,
    WrapMode,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

const APP_ID: &str = "com.quicktranslate.desktop";

// ─── Modelli Groq disponibili ───

const MODELS: &[(&str, &str)] = &[
    ("openai/gpt-oss-20b", "GPT-OSS 20B (veloce)"),
    ("llama-3.3-70b-versatile", "Llama 3.3 70B"),
    ("llama-3.1-8b-instant", "Llama 3.1 8B (velocissimo)"),
    ("gemma2-9b-it", "Gemma 2 9B"),
    ("mixtral-8x7b-32768", "Mixtral 8x7B"),
    ("deepseek-r1-distill-llama-70b", "DeepSeek R1 70B"),
];

// ─── Groq API types ───

#[derive(Debug, Serialize)]
struct GroqMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct GroqRequest {
    model: String,
    messages: Vec<GroqMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct GroqChoice {
    message: GroqMessageResponse,
}

#[derive(Debug, Deserialize)]
struct GroqMessageResponse {
    content: String,
}

#[derive(Debug, Deserialize)]
struct GroqResponse {
    choices: Vec<GroqChoice>,
}

// ─── Lingue supportate ───

const LANGUAGES: &[(&str, &str)] = &[
    ("auto", "Rileva lingua"),
    ("it", "Italiano"),
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("zh", "中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("ar", "العربية"),
    ("tr", "Türkçe"),
];

const TARGET_LANGUAGES: &[(&str, &str)] = &[
    ("it", "Italiano"),
    ("en", "English"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("zh", "中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("ar", "العربية"),
    ("tr", "Türkçe"),
];

// ─── Config ───

fn config_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config/quick-translate/.env")
}

fn save_config(api_key: &str, model: &str) {
    let target = std::env::var("TARGET_LANGUAGE").unwrap_or_else(|_| "it".to_string());
    let content = format!(
        "GROQ_API_KEY={}\nTARGET_LANGUAGE={}\nMODEL={}\n",
        api_key, target, model
    );
    let _ = std::fs::write(config_path(), content);
}

// ─── Clipboard ───

fn get_clipboard() -> Result<String> {
    let output = Command::new("wl-paste")
        .arg("--no-newline")
        .output()?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn set_clipboard(text: &str) -> Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

// ─── Traduzione ───

async fn translate_text(
    api_key: &str,
    model: &str,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<String> {
    let client = Client::new();

    let source_instruction = if source_lang == "auto" {
        "Detect the source language automatically.".to_string()
    } else {
        let name = LANGUAGES
            .iter()
            .find(|(c, _)| *c == source_lang)
            .map(|(_, n)| *n)
            .unwrap_or(source_lang);
        format!("The source language is {}.", name)
    };

    let target_name = TARGET_LANGUAGES
        .iter()
        .find(|(c, _)| *c == target_lang)
        .map(|(_, n)| *n)
        .unwrap_or("Italian");

    let request = GroqRequest {
        model: model.to_string(),
        messages: vec![
            GroqMessage {
                role: "system".to_string(),
                content: format!(
                    "You are a professional translator. {} \
                    Translate the following text to {}. \
                    Provide ONLY the translation, nothing else. \
                    Maintain the original formatting and tone.",
                    source_instruction, target_name
                ),
            },
            GroqMessage {
                role: "user".to_string(),
                content: text.to_string(),
            },
        ],
        temperature: 0.3,
        max_tokens: 4000,
    };

    let response = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("API Error: {}", error_text);
    }

    let groq_response: GroqResponse = response.json().await?;
    groq_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| anyhow::anyhow!("Nessuna traduzione ricevuta"))
}

// ─── System Tray ───

// Segnali dal tray al main thread GTK
#[derive(Debug, Clone, Copy)]
enum TrayAction {
    ToggleWindow,
    Quit,
}

struct QuickTranslateTray {
    tx: std::sync::mpsc::SyncSender<TrayAction>,
}

impl ksni::Tray for QuickTranslateTray {
    fn id(&self) -> String {
        "quick-translate".to_string()
    }

    fn title(&self) -> String {
        "Quick Translate".to_string()
    }

    fn icon_theme_path(&self) -> String {
        dirs::home_dir()
            .map(|h| h.join(".local/share/icons/hicolor/scalable/apps").to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn icon_name(&self) -> String {
        "quick-translate".to_string()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // Fallback: 22x22 icona blu/verde "QT"
        let size = 22;
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let cx = (x as f32 - size as f32 / 2.0).abs() / (size as f32 / 2.0);
                let cy = (y as f32 - size as f32 / 2.0).abs() / (size as f32 / 2.0);
                if cx * cx + cy * cy <= 1.0 {
                    // ARGB network byte order: A R G B
                    data.extend_from_slice(&[255, 53, 132, 209]); // #3584D1 blue
                } else {
                    data.extend_from_slice(&[0, 0, 0, 0]); // transparent
                }
            }
        }
        vec![ksni::Icon {
            width: size,
            height: size,
            data,
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Quick Translate".to_string(),
            description: "Traduttore veloce con Groq API".to_string(),
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: "Apri Quick Translate".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.try_send(TrayAction::ToggleWindow);
                }),
                ..Default::default()
            }),
            ksni::MenuItem::Separator,
            ksni::MenuItem::Standard(ksni::menu::StandardItem {
                label: "Esci".to_string(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.tx.try_send(TrayAction::Quit);
                }),
                ..Default::default()
            }),
        ]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.try_send(TrayAction::ToggleWindow);
    }
}

// ─── UI ───

fn build_ui(app: &Application, api_key: String, initial_model: String, tray_rx: std::sync::mpsc::Receiver<TrayAction>, quit_flag: Arc<AtomicBool>) {
    // CSS minimale: il resto lo gestisce il tema GNOME
    let css = CssProvider::new();
    css.load_from_data(
        "
        .app-title {
            font-size: 16px;
            font-weight: bold;
        }
        .app-subtitle {
            font-size: 11px;
            opacity: 0.5;
        }
        .translate-btn {
            font-weight: bold;
            padding: 8px 24px;
        }
        .panel {
            padding: 12px;
            margin: 8px;
        }
        .panel-header {
            padding: 4px 0 8px 0;
        }
        .lang-label {
            font-size: 12px;
            font-weight: bold;
            opacity: 0.7;
        }
        .status-bar {
            padding: 6px 16px;
            opacity: 0.6;
        }
        .status-text {
            font-size: 11px;
        }
        .settings-row {
            padding: 4px 0;
        }
        .settings-label {
            font-size: 12px;
            font-weight: bold;
            opacity: 0.7;
        }
        .api-entry {
            font-family: monospace;
            font-size: 12px;
        }
        "
    );

    gtk::style_context_add_provider_for_display(
        &gdk4::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Quick Translate")
        .default_width(950)
        .default_height(550)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // ═══ Header ═══
    let header = GtkBox::new(Orientation::Horizontal, 12);
    header.set_margin_start(16);
    header.set_margin_end(16);
    header.set_margin_top(12);
    header.set_margin_bottom(4);

    let title_box = GtkBox::new(Orientation::Vertical, 2);
    let title = Label::new(Some("Quick Translate"));
    title.add_css_class("app-title");
    title.set_halign(gtk::Align::Start);

    let api_key_state = Rc::new(RefCell::new(api_key.clone()));
    let model_state = Rc::new(RefCell::new(initial_model.clone()));

    let subtitle = Label::new(Some(&format!("Groq API  ·  {}",
        MODELS.iter().find(|(id, _)| *id == initial_model).map(|(_, n)| *n).unwrap_or(&initial_model)
    )));
    subtitle.add_css_class("app-subtitle");
    subtitle.set_halign(gtk::Align::Start);
    title_box.append(&title);
    title_box.append(&subtitle);
    title_box.set_hexpand(true);
    header.append(&title_box);

    let settings_btn = Button::with_label("\u{2699}");
    settings_btn.set_tooltip_text(Some("Impostazioni"));
    header.append(&settings_btn);

    main_box.append(&header);

    // ═══ Pannello impostazioni ═══
    let settings_box = GtkBox::new(Orientation::Vertical, 8);
    settings_box.set_margin_start(16);
    settings_box.set_margin_end(16);
    settings_box.set_margin_top(4);
    settings_box.set_margin_bottom(4);
    settings_box.set_visible(false);

    let model_row = GtkBox::new(Orientation::Horizontal, 8);
    model_row.add_css_class("settings-row");
    let model_label = Label::new(Some("Modello:"));
    model_label.add_css_class("settings-label");
    let model_names: Vec<&str> = MODELS.iter().map(|(_, n)| *n).collect();
    let model_list = StringList::new(&model_names);
    let model_dropdown = DropDown::new(Some(model_list), gtk::Expression::NONE);
    let initial_idx = MODELS.iter().position(|(id, _)| *id == initial_model).unwrap_or(0);
    model_dropdown.set_selected(initial_idx as u32);
    model_row.append(&model_label);
    model_row.append(&model_dropdown);
    settings_box.append(&model_row);

    let key_row = GtkBox::new(Orientation::Horizontal, 8);
    key_row.add_css_class("settings-row");
    let key_label = Label::new(Some("API Key:"));
    key_label.add_css_class("settings-label");
    let key_entry = Entry::new();
    key_entry.add_css_class("api-entry");
    key_entry.set_text(&api_key);
    key_entry.set_visibility(false);
    key_entry.set_hexpand(true);
    key_entry.set_placeholder_text(Some("gsk_..."));

    let show_key_btn = Button::with_label("Mostra");
    let save_btn = Button::with_label("Salva");
    save_btn.add_css_class("suggested-action");

    key_row.append(&key_label);
    key_row.append(&key_entry);
    key_row.append(&show_key_btn);
    key_row.append(&save_btn);
    settings_box.append(&key_row);

    main_box.append(&settings_box);

    let sep = gtk::Separator::new(Orientation::Horizontal);
    main_box.append(&sep);

    // ═══ Content area ═══
    let content = GtkBox::new(Orientation::Horizontal, 0);
    content.set_vexpand(true);

    let left_panel = GtkBox::new(Orientation::Vertical, 8);
    left_panel.add_css_class("panel");
    left_panel.set_hexpand(true);

    let left_header = GtkBox::new(Orientation::Horizontal, 8);
    left_header.add_css_class("panel-header");

    let source_lang_label = Label::new(Some("DA:"));
    source_lang_label.add_css_class("lang-label");

    let source_langs = StringList::new(&LANGUAGES.iter().map(|(_, n)| *n).collect::<Vec<_>>());
    let source_dropdown = DropDown::new(Some(source_langs), gtk::Expression::NONE);
    source_dropdown.set_selected(0);

    left_header.append(&source_lang_label);
    left_header.append(&source_dropdown);

    let clear_btn = Button::with_label("Cancella");
    clear_btn.set_halign(gtk::Align::End);
    clear_btn.set_hexpand(true);

    let paste_btn = Button::with_label("Incolla");

    left_header.append(&clear_btn);
    left_header.append(&paste_btn);

    left_panel.append(&left_header);

    let source_scroll = ScrolledWindow::new();
    source_scroll.set_vexpand(true);
    source_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    let source_text = TextView::new();
    source_text.set_wrap_mode(WrapMode::Word);
    source_text.set_left_margin(8);
    source_text.set_right_margin(8);
    source_text.set_top_margin(8);
    source_text.set_bottom_margin(8);
    source_scroll.set_child(Some(&source_text));
    left_panel.append(&source_scroll);

    content.append(&left_panel);

    let center_col = GtkBox::new(Orientation::Vertical, 8);
    center_col.set_valign(gtk::Align::Center);

    let swap_btn = Button::with_label("\u{21C4}");
    swap_btn.set_tooltip_text(Some("Scambia lingue e testo"));

    let translate_btn = Button::with_label("Traduci");
    translate_btn.add_css_class("translate-btn");
    translate_btn.add_css_class("suggested-action");

    center_col.append(&swap_btn);
    center_col.append(&translate_btn);
    content.append(&center_col);

    let right_panel = GtkBox::new(Orientation::Vertical, 8);
    right_panel.add_css_class("panel");
    right_panel.set_hexpand(true);

    let right_header = GtkBox::new(Orientation::Horizontal, 8);
    right_header.add_css_class("panel-header");

    let target_lang_label = Label::new(Some("A:"));
    target_lang_label.add_css_class("lang-label");

    let target_langs = StringList::new(&TARGET_LANGUAGES.iter().map(|(_, n)| *n).collect::<Vec<_>>());
    let target_dropdown = DropDown::new(Some(target_langs), gtk::Expression::NONE);
    target_dropdown.set_selected(0);

    right_header.append(&target_lang_label);
    right_header.append(&target_dropdown);

    let copy_btn = Button::with_label("Copia");
    copy_btn.set_halign(gtk::Align::End);
    copy_btn.set_hexpand(true);

    right_header.append(&copy_btn);

    right_panel.append(&right_header);

    let target_scroll = ScrolledWindow::new();
    target_scroll.set_vexpand(true);
    target_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    let target_text = TextView::new();
    target_text.set_wrap_mode(WrapMode::Word);
    target_text.set_editable(false);
    target_text.set_left_margin(8);
    target_text.set_right_margin(8);
    target_text.set_top_margin(8);
    target_text.set_bottom_margin(8);
    target_scroll.set_child(Some(&target_text));
    right_panel.append(&target_scroll);

    content.append(&right_panel);
    main_box.append(&content);

    // ═══ Status bar ═══
    let status_bar = GtkBox::new(Orientation::Horizontal, 8);
    status_bar.add_css_class("status-bar");
    let status_label = Label::new(Some("Pronto  ·  Ctrl+Enter per tradurre  ·  Ctrl+Alt+G per traduzione rapida"));
    status_label.add_css_class("status-text");
    status_label.set_halign(gtk::Align::Start);
    status_label.set_hexpand(true);
    status_bar.append(&status_label);

    let char_count = Label::new(Some("0 caratteri"));
    char_count.add_css_class("status-text");
    status_bar.append(&char_count);

    main_box.append(&status_bar);
    window.set_child(Some(&main_box));

    // ═══ Logica ═══

    // Conta caratteri
    let char_count_clone = char_count.clone();
    let source_buffer = source_text.buffer();
    source_buffer.connect_changed(move |buf| {
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
        char_count_clone.set_text(&format!("{} caratteri", text.len()));
    });

    // Toggle impostazioni
    let settings_box_clone = settings_box.clone();
    settings_btn.connect_clicked(move |_| {
        let visible = settings_box_clone.is_visible();
        settings_box_clone.set_visible(!visible);
    });

    // Mostra/nascondi API key
    let key_entry_clone = key_entry.clone();
    show_key_btn.connect_clicked(move |btn| {
        if gtk4::prelude::EntryExt::is_visible(&key_entry_clone) {
            key_entry_clone.set_visibility(false);
            btn.set_label("Mostra");
        } else {
            key_entry_clone.set_visibility(true);
            btn.set_label("Nascondi");
        }
    });

    // Cambio modello
    let model_state_clone = model_state.clone();
    let subtitle_clone = subtitle.clone();
    let status_label_clone = status_label.clone();
    model_dropdown.connect_selected_notify(move |dd| {
        let idx = dd.selected() as usize;
        if idx < MODELS.len() {
            let (id, name) = MODELS[idx];
            *model_state_clone.borrow_mut() = id.to_string();
            subtitle_clone.set_text(&format!("Groq API  ·  {}", name));
            status_label_clone.set_text(&format!("Modello cambiato: {}", name));
        }
    });

    // Salva impostazioni
    let api_key_state_clone = api_key_state.clone();
    let model_state_clone = model_state.clone();
    let key_entry_clone = key_entry.clone();
    let status_label_clone = status_label.clone();
    save_btn.connect_clicked(move |_| {
        let new_key = key_entry_clone.text().to_string();
        let current_model = model_state_clone.borrow().clone();
        if new_key.is_empty() {
            status_label_clone.set_text("Errore: API key non può essere vuota");
            return;
        }
        *api_key_state_clone.borrow_mut() = new_key.clone();
        save_config(&new_key, &current_model);
        status_label_clone.set_text("Impostazioni salvate!");
    });

    // Cancella
    let source_text_clone = source_text.clone();
    let target_text_clone = target_text.clone();
    clear_btn.connect_clicked(move |_| {
        source_text_clone.buffer().set_text("");
        target_text_clone.buffer().set_text("");
    });

    // Incolla
    let source_text_clone = source_text.clone();
    paste_btn.connect_clicked(move |_| {
        if let Ok(text) = get_clipboard() {
            source_text_clone.buffer().set_text(&text);
        }
    });

    // Copia
    let target_text_clone = target_text.clone();
    copy_btn.connect_clicked(move |_| {
        let buf = target_text_clone.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
        let _ = set_clipboard(&text);
    });

    // Swap
    let source_text_clone = source_text.clone();
    let target_text_clone = target_text.clone();
    let source_dropdown_clone = source_dropdown.clone();
    let target_dropdown_clone = target_dropdown.clone();
    swap_btn.connect_clicked(move |_| {
        let src_buf = source_text_clone.buffer();
        let tgt_buf = target_text_clone.buffer();
        let src_text = src_buf.text(&src_buf.start_iter(), &src_buf.end_iter(), false);
        let tgt_text = tgt_buf.text(&tgt_buf.start_iter(), &tgt_buf.end_iter(), false);

        src_buf.set_text(&tgt_text);
        tgt_buf.set_text(&src_text);

        let tgt_sel = target_dropdown_clone.selected();
        let src_sel = source_dropdown_clone.selected();

        if src_sel > 0 {
            target_dropdown_clone.set_selected(src_sel - 1);
        }
        source_dropdown_clone.set_selected(tgt_sel + 1);
    });

    // Traduci
    let source_text_clone = source_text.clone();
    let target_text_clone = target_text.clone();
    let source_dropdown_clone = source_dropdown.clone();
    let target_dropdown_clone = target_dropdown.clone();
    let status_label_clone = status_label.clone();
    let api_key_state_clone = api_key_state.clone();
    let model_state_clone = model_state.clone();
    let translate_btn_clone = translate_btn.clone();

    translate_btn.connect_clicked(move |_| {
        let buf = source_text_clone.buffer();
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);

        if text.trim().is_empty() {
            status_label_clone.set_text("Inserisci del testo da tradurre");
            return;
        }

        let src_idx = source_dropdown_clone.selected() as usize;
        let tgt_idx = target_dropdown_clone.selected() as usize;

        let source_code = LANGUAGES[src_idx].0.to_string();
        let target_code = TARGET_LANGUAGES[tgt_idx].0.to_string();
        let current_api_key = api_key_state_clone.borrow().clone();
        let current_model = model_state_clone.borrow().clone();

        status_label_clone.set_text("Traduzione in corso...");
        translate_btn_clone.set_sensitive(false);
        translate_btn_clone.set_label("...");

        let target_text_ref = target_text_clone.clone();
        let status_ref = status_label_clone.clone();
        let btn_ref = translate_btn_clone.clone();
        let text_owned = text.to_string();

        let (tx, rx) = std::sync::mpsc::channel::<Result<String, String>>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(translate_text(
                &current_api_key,
                &current_model,
                &text_owned,
                &source_code,
                &target_code,
            ));
            match result {
                Ok(translation) => { let _ = tx.send(Ok(translation)); }
                Err(e) => { let _ = tx.send(Err(e.to_string())); }
            }
        });

        glib::idle_add_local_once(move || {
            let target_text_ref2 = target_text_ref.clone();
            let status_ref2 = status_ref.clone();
            let btn_ref2 = btn_ref.clone();

            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                match rx.try_recv() {
                    Ok(Ok(translation)) => {
                        let _ = set_clipboard(&translation);
                        target_text_ref2.buffer().set_text(&translation);
                        status_ref2.set_text("Traduzione completata  ·  Copiata nella clipboard");
                        btn_ref2.set_sensitive(true);
                        btn_ref2.set_label("Traduci");
                        glib::ControlFlow::Break
                    }
                    Ok(Err(e)) => {
                        status_ref2.set_text(&format!("Errore: {}", e));
                        btn_ref2.set_sensitive(true);
                        btn_ref2.set_label("Traduci");
                        glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        glib::ControlFlow::Continue
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        status_ref2.set_text("Errore: connessione persa");
                        btn_ref2.set_sensitive(true);
                        btn_ref2.set_label("Traduci");
                        glib::ControlFlow::Break
                    }
                }
            });
        });
    });

    // Ctrl+Enter per tradurre
    let key_controller = gtk::EventControllerKey::new();
    let translate_btn_ref = translate_btn.clone();
    key_controller.connect_key_pressed(move |_, key, _, modifier| {
        if key == gdk4::Key::Return
            && modifier.contains(gdk4::ModifierType::CONTROL_MASK)
        {
            translate_btn_ref.emit_clicked();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_controller);

    // ═══ Chiudi → nascondi nel tray ═══
    let quit_flag_clone = quit_flag.clone();
    window.connect_close_request(move |w| {
        if quit_flag_clone.load(Ordering::Relaxed) {
            // Esci davvero
            glib::Propagation::Proceed
        } else {
            // Nascondi nel tray
            w.set_visible(false);
            glib::Propagation::Stop
        }
    });

    // ═══ Ascolta azioni dal tray ═══
    let win_for_tray = window.clone();
    let app_for_tray = app.clone();
    let quit_flag_for_tray = quit_flag.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        match tray_rx.try_recv() {
            Ok(TrayAction::ToggleWindow) => {
                if win_for_tray.is_visible() {
                    win_for_tray.set_visible(false);
                } else {
                    win_for_tray.set_visible(true);
                    win_for_tray.present();
                }
            }
            Ok(TrayAction::Quit) => {
                quit_flag_for_tray.store(true, Ordering::Relaxed);
                app_for_tray.quit();
                return glib::ControlFlow::Break;
            }
            Err(_) => {}
        }
        glib::ControlFlow::Continue
    });

    // Avvia nascosto (parte nel tray)
    window.set_visible(false);
}

fn main() {
    // Carica .env
    let config_env = dirs::home_dir()
        .map(|h| h.join(".config/quick-translate/.env"));

    if let Some(ref path) = config_env {
        if path.exists() {
            dotenv::from_path(path).ok();
        } else {
            dotenv::dotenv().ok();
        }
    } else {
        dotenv::dotenv().ok();
    }

    let api_key = std::env::var("GROQ_API_KEY").expect(
        "GROQ_API_KEY non trovata! Crea un file .env con GROQ_API_KEY=la_tua_chiave",
    );

    let model = std::env::var("MODEL").unwrap_or_else(|_| "openai/gpt-oss-20b".to_string());

    // Canale tray → GTK
    let (tray_tx, tray_rx) = std::sync::mpsc::sync_channel::<TrayAction>(8);

    // Flag per uscita reale
    let quit_flag = Arc::new(AtomicBool::new(false));

    // Avvia tray in thread separato
    let tray = QuickTranslateTray { tx: tray_tx };
    use ksni::blocking::TrayMethods;
    let _tray_handle = tray.spawn().expect("Impossibile avviare il system tray");

    // Avvolgi il receiver in Rc<RefCell<Option>> per passarlo una volta sola
    let tray_rx_holder: Rc<RefCell<Option<std::sync::mpsc::Receiver<TrayAction>>>> =
        Rc::new(RefCell::new(Some(tray_rx)));

    let app = Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::empty())
        .build();

    // Teniamo l'app viva anche senza finestre visibili
    app.set_accels_for_action("app.quit", &["<Control>q"]);

    let api_key_clone = api_key.clone();
    let model_clone = model.clone();
    let quit_flag_clone = quit_flag.clone();
    let tray_rx_clone = tray_rx_holder.clone();
    app.connect_activate(move |app| {
        // Se la finestra esiste già, mostrala
        if let Some(win) = app.active_window() {
            win.set_visible(true);
            win.present();
            return;
        }
        // Prendi il receiver (una volta sola)
        let rx = tray_rx_clone.borrow_mut().take().expect("tray_rx già consumato");
        build_ui(app, api_key_clone.clone(), model_clone.clone(), rx, quit_flag_clone.clone());
    });

    // Impedisci che l'app esca quando la finestra viene nascosta
    app.set_property("register-session", true);

    app.run_with_args::<String>(&[]);
}
