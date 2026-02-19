# Quick Translate

Alternativa a DeepL per Linux (Wayland/GNOME) scritta in Rust con GTK4. Usa le API Groq per traduzioni ultra-veloci.

## Caratteristiche

- **Popup rapido** - Copia del testo, premi `Ctrl+Alt+G`, ottieni la traduzione in un popup vicino al cursore
- **App desktop completa** - Interfaccia a due pannelli con testo sorgente/tradotto, selettori lingua, scelta modello
- **System tray** - L'app vive nel tray di sistema, sempre pronta all'uso (~41 MB di RAM propria, ~0% CPU a riposo)
- **Tema GNOME** - Segue automaticamente il tema di sistema (chiaro/scuro)
- **Wayland nativo** - Usa `wl-paste`/`wl-copy` per la clipboard, nessuna dipendenza da X11
- **Modello selezionabile** - Scegli tra diversi modelli Groq direttamente dall'interfaccia
- **Ctrl+Enter** - Scorciatoia per tradurre nell'app desktop

## Modelli disponibili

| Modello | Note |
|---------|------|
| GPT-OSS 20B | Veloce (default) |
| Llama 3.3 70B | Versatile |
| Llama 3.1 8B | Velocissimo |
| Gemma 2 9B | Compatto |
| Mixtral 8x7B | Bilanciato |
| DeepSeek R1 70B | Ragionamento |

## Prerequisiti

- **Rust** - `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **GTK4 dev** - `sudo apt install libgtk-4-dev`
- **libayatana-appindicator** - `sudo apt install libayatana-appindicator3-dev` (per il system tray)
- **wl-clipboard** - `sudo apt install wl-clipboard`
- **Account Groq** - Gratuito su https://console.groq.com

## Installazione

```bash
# Clona il progetto
git clone <repo-url>
cd quick-translate-rust

# Configura l'API key
mkdir -p ~/.config/quick-translate
echo "GROQ_API_KEY=gsk_la_tua_chiave" > ~/.config/quick-translate/.env
echo "TARGET_LANGUAGE=it" >> ~/.config/quick-translate/.env
echo "MODEL=openai/gpt-oss-20b" >> ~/.config/quick-translate/.env

# Compila e installa
./install.sh
```

Oppure manualmente:

```bash
cargo build --release
cp target/release/quick-translate ~/.local/bin/
cp target/release/quick-translate-popup ~/.local/bin/
```

## Utilizzo

### App desktop

```bash
quick-translate
```

L'app si avvia nel system tray. Clicca sull'icona per aprire/chiudere la finestra. Chiudere la finestra con la X la nasconde nel tray.

- Scrivi o incolla del testo nel pannello sinistro
- Seleziona lingua sorgente e destinazione
- Premi **Traduci** o **Ctrl+Enter**
- La traduzione appare a destra e viene copiata nella clipboard
- Clicca l'icona ingranaggio per cambiare modello o API key

### Popup rapido (Ctrl+Alt+G)

1. Copia del testo in qualsiasi applicazione (`Ctrl+C`)
2. Premi **Ctrl+Alt+G**
3. La traduzione appare in un popup e viene copiata nella clipboard
4. Incolla con `Ctrl+V`

La scorciatoia va configurata manualmente in GNOME:

```
Impostazioni > Tastiera > Scorciatoie personalizzate
Nome: Quick Translate Popup
Comando: /home/TUO_UTENTE/.local/bin/quick-translate-popup
Scorciatoia: Ctrl+Alt+G
```

## Configurazione

Il file di configurazione si trova in `~/.config/quick-translate/.env`:

```env
GROQ_API_KEY=gsk_la_tua_chiave
TARGET_LANGUAGE=it
MODEL=openai/gpt-oss-20b
```

Le impostazioni possono anche essere modificate direttamente dall'interfaccia dell'app (pulsante ingranaggio).

## Struttura del progetto

```
quick-translate-rust/
├── Cargo.toml          # Dipendenze e configurazione binari
├── install.sh          # Script di build e installazione
├── src/
│   ├── main.rs         # Popup rapido (one-shot, triggerato da Ctrl+Alt+G)
│   └── app.rs          # App desktop con system tray
```

### File installati

| File | Descrizione |
|------|-------------|
| `~/.local/bin/quick-translate` | App desktop |
| `~/.local/bin/quick-translate-popup` | Popup rapido |
| `~/.config/quick-translate/.env` | Configurazione |
| `~/.local/share/applications/quick-translate.desktop` | Launcher applicazioni |
| `~/.config/autostart/quick-translate.desktop` | Avvio automatico |
| `~/.local/share/icons/hicolor/scalable/apps/quick-translate.svg` | Icona |

## Consumo risorse

| Metrica | Valore |
|---------|--------|
| CPU a riposo | ~0% |
| RAM propria | ~41 MB |
| RAM condivisa (GTK) | ~89 MB (condivisa con altre app GNOME) |
| Swap | 0 MB |

## Lingue supportate

Italiano, English, Espanol, Francais, Deutsch, Portugues, Russkij, Zhongwen, Nihongo, Hangugeo, Nederlands, Polski, Svenska, Arabiyya, Turkce

## Licenza

MIT
