# Viima 🦀🌐

Moderni, nopea ja tietoturvallinen suomalainen työpöytäselain **IT-Veljekset Groupilta**, joka on rakennettu **Rustilla** ja **WebView2 / Wry / Tao** -arkkitehtuurilla.

![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)
![Platform](https://img.shields.io/badge/Platform-Windows-blue.svg)
![Status](https://img.shields.io/badge/Status-Valmis%20k%C3%A4ytett%C3%A4v%C3%A4ksi-green.svg)

---

## 🌟 Ominaisuudet

- **⚡ Täysiverinen selauskokemus**: Avaa minkä tahansa nykyaikaisen verkkosivuston (YouTube, Google, GitHub, Yle, verkkopankit, WebGL, videot ja äänet) huippunopealla suorituskyvyllä.
- **📑 Monivälilehtijärjestelmä (Tabs)**:
  - Luo uusia välilehtiä (`+` tai `Ctrl+T`)
  - Sulje välilehtiä (`×` tai `Ctrl+W`)
  - Vaihda välilehteä klikkaamalla. Jokaisella välilehdellä säilyy oma tila, selaushistoria ja DOM.
- **🔍 Älykäs osoite- ja hakurivi (Smart Omnibox)**:
  - Tunnistaa automaattisesti syötetäänkö suora URL vai hakusana.
  - Tuki bang-oikopoluille:
    - `!g hakusana` -> Google
    - `!d hakusana` -> DuckDuckGo
    - `!w hakusana` -> Wikipedia
    - `!yt hakusana` -> YouTube
    - `!gh hakusana` -> GitHub
    - `!yle hakusana` -> Yle Uutiset
  - SSL-suojauksen ilmaisin (`🔒`).
- **🛡️ Sisäänrakennettu mainos- ja seurantasuodatin (AdBlock)**:
  - Estää tunnettuja mainos- ja seurantaverkkotunnuksia sivunavigoinnissa.
  - Kosmeettinen suodatus piilottaa osan mainospaikoista sivuilla.
  - Päällä/pois -kytkin suoraan yläpalkissa.
- **⭐ Kirjanmerkit ja selaushistoria**:
  - Lisää aktiivinen sivu kirjanmerkkeihin yhdellä klikkauksella (`★` tai `Ctrl+D`).
  - Tallennettujen kirjanmerkkien ja historian selaus, haku ja poisto selaimen sisäsivuilla.
  - Pysyvä tallennus käyttäjän AppData-hakemistoon (`%APPDATA%\IT-Veljekset\Viima\config\browser_data.json`).
- **🏠 Muokattava aloitussivu (`browser://newtab`)**:
  - Koko ruudun widget-ruudukko: haku, kello, sää (Open-Meteo), pörssikurssit (Yahoo Finance) ja pikalinkit kirjanmerkeistä.
  - "Muokkaa"-painikkeella widgettejä voi lisätä, poistaa, siirtää ja leventää. Kellon ulkoasu (digitaalinen/analoginen, fontti, koko, väri) on säädettävissä.
  - Valittavat taustakuvat (Aurora, Glass Waves, Nordic Light).
- **🖼️ Oikeat faviconit**: sivustojen ikonit haetaan ja tallennetaan välimuistiin, joten ne näkyvät välilehdissä, kirjanmerkeissä ja aloitussivulla myös uudelleenkäynnistyksen jälkeen.
- **🗂️ Sisäsivut omissa välilehdissään**: kirjanmerkit, historia, lataukset ja asetukset avautuvat uuteen välilehteen (jo auki oleva käytetään uudelleen).
- **📦 Windows-asennusohjelma ja automaattiset päivitykset**:
  - Käyttäjäkohtainen asennus kansioon `%LOCALAPPDATA%\IT-Veljekset\Viima` (`Viima-Setup.exe`, ei järjestelmänvalvojan oikeuksia, `--silent`-tuki, Käynnistä-valikon ja työpöydän pikakuvakkeet sekä Windowsin Sovellukset ja ominaisuudet -poistomerkintä).
  - GitHub Releases -pohjainen automaattinen päivitys (`latest.json` + SHA-256-varmennus ja automaattinen binäärin vaihto uudelleenkäynnistyksen yhteydessä).

---

## ⌨️ Pikanäppäimet

| Näppäinyhdistelmä | Toiminto |
|---|---|
| `Ctrl + T` | Avaa uusi välilehti |
| `Ctrl + W` | Sulje nykyinen välilehti |
| `Ctrl + L` | Aktivoi ja valitse osoiterivi |
| `Ctrl + R` tai `F5` | Päivitä sivu |
| `Ctrl + D` | Lisää tai poista nykyinen sivu kirjanmerkkeihin |
| `Ctrl + H` | Avaa selaushistoria (uuteen välilehteen) |
| `Ctrl + J` | Avaa lataukset (uuteen välilehteen) |
| `Ctrl + F` | Hae sivulta |
| `Ctrl + Shift + T` | Avaa viimeksi suljettu välilehti |
| `Ctrl + Shift + N` | Uusi yksityinen ikkuna |
| `Ctrl + Shift + B` | Näytä tai piilota kirjanmerkkipalkki |
| `Ctrl + 1…9` | Siirry välilehteen |
| `Ctrl + + / - / 0` | Zoomaa lähemmäs / kauemmas / palauta |
| `Alt + Vasen` | Siirry taaksepäin historiassa |
| `Alt + Oikea` | Siirry eteenpäin historiassa |

---

## 🚀 Käynnistys ja kääntäminen

### Esivaatimukset
- **Rust** (1.85+; edition 2024)
- **Windows 10 / 11** (sisältää valmiiksi Microsoft Edge WebView2 Runtimen)

### Kehitystilassa ajaminen
```bash
cargo run
```

### Tuotantoversion ja asennusohjelman kääntäminen
Suositeltu tapa kääntää koko tuotantopaketti (`Viima.exe`, `Viima-Setup.exe`, `Viima_v*_Windows_x64.zip` ja `latest.json` kansioihin `releases/latest/` ja `releases/v<versio>/`):
```powershell
.\scripts\build_release.ps1
```
Voit ajaa pelkän selaimen release-käännöksen myös komennolla:
```bash
cargo build --release --bin viima
```
Asennusohjelma tukee myös hiljaista asennusta (`Viima-Setup.exe --silent`).

---

## 📁 Projektirakenne

```text
viima/
├── Cargo.toml                  # Riippuvuudet (wry, tao, rusqlite, ureq, ed25519-dalek, windows)
├── build.rs                    # Exe-ikoni ja asennusohjelman upotettu binääri
├── .github/workflows/
│   └── release.yml             # Tagista (v*) käännös, allekirjoitus ja GitHub Release
├── scripts/
│   └── build_release.ps1       # Julkaisupaketin, asennusohjelman ja latest.json-luonti (CI käyttää tätä)
├── src/
│   ├── main.rs                 # Sovelluksen sisääntulo ja komentorivi
│   ├── app/
│   │   ├── mod.rs              # Selaimen tila ja tapahtumasilmukka
│   │   ├── ipc.rs              # Käyttöliittymän ja sisäsivujen viestit, luottamusrajat
│   │   ├── protocol.rs         # browser://-sisäsivujen tarjoilu ja CSP
│   │   ├── tabs.rs             # Välilehtien luonti ja istunnon palautus
│   │   ├── features.rs         # Valikot, zoom, lataukset, asetukset, yksityinen ikkuna
│   │   └── remote.rs           # Faviconit ja widgetien (sää, kurssit) verkkohaut
│   ├── tab.rs                  # Välilehden tila
│   ├── storage.rs              # Kirjanmerkit, asetukset ja istunto (JSON)
│   ├── history_db.rs           # Selaushistoria (SQLite)
│   ├── favicon.rs              # Faviconien haku ja välimuisti (vain julkiset osoitteet)
│   ├── widgets.rs              # Aloitussivun widgetien sallitut pyyntömallit
│   ├── adblock.rs              # Mainos- ja seurantasuodatin
│   ├── url_resolver.rs         # Osoitteiden ja hakukoneiden ratkaisija
│   ├── updater.rs, update_sig.rs  # Päivitykset ja allekirjoituksen tarkistus
│   ├── webview_hooks.rs        # WebView2:n lataukset, evästeet ja äänet
│   ├── win_platform.rs, win_shell.rs  # Windows-integraatio (rekisteri, valikot, pikakuvakkeet)
│   ├── i18n.rs                 # Rust-puolen tekstit (fi/en)
│   └── bin/
│       ├── setup.rs            # Asennusohjelma (Viima-Setup.exe)
│       └── sign.rs             # Päivityksen allekirjoitus (julkaisuputki)
└── assets/
    ├── branding/               # Logo ja kuvakkeet
    ├── inject/                 # Sivuille ruiskutettavat skriptit (favicon, sivulta haku)
    ├── i18n.js, favicons.js    # Sisäsivujen yhteiset skriptit
    ├── ui/                     # Yläpalkki (index.html, style.css, app.js)
    └── internal/               # newtab, history, bookmarks, downloads, settings, insecure + taustakuvat
```
---

## 🛡️ Tietoturva

- **SmartScreen päällä**: Edgen phishing- ja haittaohjelmavaroitukset ovat käytössä.
- **Moottori päivittyy itsestään**: verkkosivut ajetaan Edgen WebView2 Runtimessa, jonka Windows päivittää.
- **Osoiterivi ei valehtele**: vain Viiman omat sisäsivut näytetään `browser://`-nimellä, muut osoitteet näkyvät sellaisenaan. `http://`-sivut saavat punaisen varoituskuvakkeen.
- **Sisäsivut suojattu**: web-sivu ei voi upottaa asetus- tai historiasivuja (clickjacking) eikä lähettää selaimelle muita komentoja kuin pikanäppäimiä.
- **`window.open`/`target=_blank`** avaa vain `http(s)`-osoitteita, ei `file://`-osoitteita.
- Kirjautumiset säilyvät uudelleenkäynnistyksen yli: `%LOCALAPPDATA%\IT-Veljekset\Viima\data\WebView2` (kuten Edgessä). Poista kansio, jos haluat kirjautua kaikkialta ulos.
- Huomio: selain on uusi eikä sisällä salasananhallintaa tai laajennuksia. Google voi estää kirjautumisen upotetuissa selaimissa.

## 🔒 Yksityisyys ja tietosuoja

Viima ei lähetä telemetriaa tai selaustietoja kolmansille osapuolille. Kaikki selaushistoria ja kirjanmerkit säilytetään vain ja ainoastaan käyttäjän omalla laitteella.

Aloitussivun widgetit tekevät pyyntöjä vain, kun ne ovat käytössä: sääwidget lähettää valitun paikkakunnan nimen ja koordinaatit Open-Meteolle, pörssiwidget kurssitunnukset Yahoo Financelle, ja faviconit haetaan sivustoilta itseltään (vain julkisista osoitteista). Poista widget aloitussivulta, jos et halua näitä pyyntöjä.
