use anyhow::Result;
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, Label, Box as GtkBox, Orientation, CssProvider};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "com.quicktranslate.app";

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

/// Legge il testo dalla clipboard usando wl-paste (Wayland)
fn get_clipboard() -> Result<String> {
    let output = Command::new("wl-paste")
        .arg("--no-newline")
        .output()?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8(output.stdout)?)
}

/// Scrive il testo nella clipboard usando wl-copy (Wayland)
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

/// Ottiene la posizione del cursore del mouse
fn get_mouse_position() -> (i32, i32) {
    // Su Wayland usiamo slurp o un fallback
    // Proviamo con gdbus per ottenere la posizione del puntatore
    let output = Command::new("bash")
        .arg("-c")
        .arg("gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval 'global.get_pointer()' 2>/dev/null")
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output formato: (true, '[x, y, mods]')
        if let Some(start) = stdout.find('[') {
            if let Some(end) = stdout.find(']') {
                let coords = &stdout[start+1..end];
                let parts: Vec<&str> = coords.split(',').collect();
                if parts.len() >= 2 {
                    let x = parts[0].trim().parse::<i32>().unwrap_or(100);
                    let y = parts[1].trim().parse::<i32>().unwrap_or(100);
                    return (x, y);
                }
            }
        }
    }

    // Fallback: centro dello schermo
    (500, 400)
}

fn translate_blocking(api_key: &str, text: &str, target_lang: &str) -> Result<String> {
    let target_lang_name = match target_lang {
        "it" => "Italian",
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "zh" => "Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        _ => "Italian",
    };

    let request = GroqRequest {
        model: "openai/gpt-oss-20b".to_string(),
        messages: vec![
            GroqMessage {
                role: "system".to_string(),
                content: format!(
                    "You are a professional translator. Detect the source language. \
                    If the text is already in {target}, translate it to English. \
                    Otherwise translate it to {target}. \
                    Provide ONLY the translation, nothing else. \
                    Maintain the original formatting and tone.",
                    target = target_lang_name
                ),
            },
            GroqMessage {
                role: "user".to_string(),
                content: text.to_string(),
            },
        ],
        temperature: 0.3,
        max_tokens: 2000,
    };

    // Usa una runtime tokio temporanea per la chiamata HTTP
    let rt = tokio::runtime::Runtime::new()?;
    let translation = rt.block_on(async {
        let client = Client::new();
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
    })?;

    Ok(translation)
}

fn build_ui(app: &Application, translation: &str, mouse_x: i32, mouse_y: i32) {
    // CSS minimale: usa il tema GNOME di sistema
    let css = CssProvider::new();
    css.load_from_data(
        "
        .popup-box {
            border-radius: 12px;
            padding: 16px 20px;
        }
        .header-label {
            font-size: 11px;
            font-weight: bold;
            letter-spacing: 1px;
            opacity: 0.6;
        }
        .translation-text {
            font-size: 14px;
            margin-top: 8px;
        }
        .hint-label {
            font-size: 10px;
            margin-top: 8px;
            opacity: 0.5;
        }
        .copy-button {
            margin-top: 8px;
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
        .default_width(380)
        .decorated(false)
        .resizable(false)
        .build();

    // Rendi la finestra un popup flottante
    window.set_default_size(380, -1);

    // Container principale
    let main_box = GtkBox::new(Orientation::Vertical, 4);
    main_box.add_css_class("popup-box");

    // Header
    let header = Label::new(Some("QUICK TRANSLATE"));
    header.add_css_class("header-label");
    header.set_halign(gtk::Align::Start);
    main_box.append(&header);

    // Testo tradotto
    let text_label = Label::new(Some(translation));
    text_label.add_css_class("translation-text");
    text_label.set_halign(gtk::Align::Start);
    text_label.set_wrap(true);
    text_label.set_max_width_chars(45);
    text_label.set_selectable(true);
    main_box.append(&text_label);

    // Riga in basso con hint e bottone copia
    let bottom_box = GtkBox::new(Orientation::Horizontal, 8);

    let hint = Label::new(Some("Ctrl+V per incollare  |  Esc per chiudere"));
    hint.add_css_class("hint-label");
    hint.set_halign(gtk::Align::Start);
    hint.set_hexpand(true);
    bottom_box.append(&hint);

    let copy_button = gtk::Button::with_label("Copiato!");
    copy_button.add_css_class("copy-button");
    copy_button.set_sensitive(false);
    bottom_box.append(&copy_button);

    main_box.append(&bottom_box);
    window.set_child(Some(&main_box));

    // Chiudi con Escape o click fuori
    let key_controller = gtk::EventControllerKey::new();
    let win_clone = window.clone();
    key_controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk4::Key::Escape {
            win_clone.close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_controller);

    // Chiudi automaticamente in base alla lunghezza del testo
    // ~200 parole/minuto = ~3.3 parole/secondo, minimo 5 secondi
    let word_count = translation.split_whitespace().count() as u32;
    let seconds = (word_count / 3).max(5);
    let win_clone = window.clone();
    glib::timeout_add_seconds_local_once(seconds, move || {
        win_clone.close();
    });

    // Focus lost = chiudi
    let focus_controller = gtk::EventControllerFocus::new();
    let win_clone = window.clone();
    focus_controller.connect_leave(move |_| {
        let w = win_clone.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            w.close();
        });
    });
    window.add_controller(focus_controller);

    window.present();

    // Posiziona la finestra dopo che è stata mostrata
    // Su GTK4/Wayland non possiamo posizionare direttamente,
    // ma la finestra apparirà comunque come popup sopra le altre
    let _ = (mouse_x, mouse_y); // Su Wayland il posizionamento è gestito dal compositor
}

fn main() {
    // Carica .env: prima da ~/.config/quick-translate/.env, poi fallback
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

    let api_key = match std::env::var("GROQ_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            eprintln!("GROQ_API_KEY non configurata!");
            return;
        }
    };

    let target_lang = std::env::var("TARGET_LANGUAGE").unwrap_or_else(|_| "it".to_string());

    // Ottieni posizione mouse PRIMA di tutto
    let (mouse_x, mouse_y) = get_mouse_position();

    // Leggi clipboard
    let text = match get_clipboard() {
        Ok(t) if !t.trim().is_empty() => t,
        _ => {
            eprintln!("Clipboard vuota!");
            return;
        }
    };

    // Traduci (blocking, prima di avviare GTK)
    let translation = match translate_blocking(&api_key, &text, &target_lang) {
        Ok(t) => {
            // Copia nella clipboard
            let _ = set_clipboard(&t);
            t
        }
        Err(e) => {
            eprintln!("Errore traduzione: {}", e);
            return;
        }
    };

    // Avvia GTK e mostra popup
    let translation = Rc::new(RefCell::new(translation));
    let app = Application::builder()
        .application_id(APP_ID)
        .build();

    let translation_clone = translation.clone();
    app.connect_activate(move |app| {
        let t = translation_clone.borrow();
        build_ui(app, &t, mouse_x, mouse_y);
    });

    app.run_with_args::<String>(&[]);
}
