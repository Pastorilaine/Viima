# Viima (ent. Oma Selain) 🦀🌐

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
  - Pysyvä tallennus käyttäjän AppData-hakemistoon (`%APPDATA%\IT-Veljekset\OmaSelain\config\browser_data.json`).
- **🏠 Moderni suomenkielinen aloitussivu (`browser://newtab`)**:
  - Reaaliaikainen digitaalikello ja päivämäärä suomeksi.
  - Vuorokaudenaikaan mukautuva tervehdys ja valittavat taustakuvat (Aurora, Glass Waves, Nordic Light).
  - Tyylikäs pikahakukenttä ja pikalinkit suosituimmille sivustoille.
- **📦 Windows-asennusohjelma ja automaattiset päivitykset**:
  - Käyttäjäkohtainen asennus kansioon `%LOCALAPPDATA%\IT-Veljekset\OmaSelain` (`OmaSelain-Setup.exe`, ei järjestelmänvalvojan oikeuksia, `--silent`-tuki, Käynnistä-valikon ja työpöydän pikakuvakkeet sekä Windowsin Sovellukset ja ominaisuudet -poistomerkintä).
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
| `Ctrl + H` | Avaa selaushistoria |
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
Suositeltu tapa kääntää koko tuotantopaketti (`OmaSelain.exe`, `OmaSelain-Setup.exe`, `OmaSelain_v*_Windows_x64.zip` ja `latest.json` kansioihin `releases/latest/` ja `releases/v<versio>/`):
```powershell
.\scripts\build_release.ps1
```
Voit ajaa pelkän selaimen release-käännöksen myös komennolla:
```bash
cargo build --release --bin oma_selain
```
Asennusohjelma tukee myös hiljaista asennusta (`OmaSelain-Setup.exe --silent`).

---

## 📁 Projektirakenne

```text
oma-selain/
├── Cargo.toml                  # Riippuvuudet (wry, tao, serde, sha2, ureq, windows)
├── build.rs                    # Asennusohjelman upotetun binäärin kääntölinkitys
├── scripts/
│   └── build_release.ps1       # Julkaisupaketin, asennusohjelman ja latest.json-luonti
├── releases/
│   ├── latest/                 # Uusin valmis julkaisu (Setup.exe, Exe, Zip, latest.json)
│   ├── v0.3.0/                 # Versioitu julkaisukansio
│   └── archive/                # Vanhemmat ZIP-julkaisut (v0.1.0 - v0.2.1)
├── src/
│   ├── main.rs                 # Sovelluksen sisääntulo ja CLI-päivitysliput
│   ├── app.rs                  # Selaimen tilakoordinaattori, IPC-viestintä ja päivitykset
│   ├── tab.rs                  # Välilehden elinkaari ja navigaatiohistoria
│   ├── storage.rs              # Kirjanmerkkien, historian ja asetusten tallennus
│   ├── adblock.rs              # Mainosestolistan ja skripti-injektion moottori
│   ├── url_resolver.rs         # Osoitteiden ja hakukoneiden ratkaisija
│   ├── updater.rs              # GitHub Releases -päivitystarkistus, SHA-256 ja --uninstall
│   ├── win_platform.rs         # Windows-pikakuvakkeet, rekisteri ja WebView2-tarkistus
│   └── bin/setup.rs            # Natiivi Windows-asennusohjelma (OmaSelain-Setup.exe)
├── assets/
│   ├── branding/               # Viima-logo (.png, .ico, 64x64 .rgba)
│   ├── ui/
│   │   ├── index.html          # Yläpalkin käyttöliittymä
│   │   ├── style.css           # Moderni tumma ja vaalea teema
│   │   └── app.js              # UI:n reaktiivinen logiikka ja IPC
│   └── internal/
│       ├── newtab.html         # Aloitussivu (kello, haku, pikalinkit, taustakuvat)
│       ├── history.html        # Selaushistorian hallinta
│       ├── bookmarks.html      # Kirjanmerkkien hallinta
│       ├── settings.html       # Selaimen asetukset ja päivitysten hallinta
│       └── wallpapers/         # Aloitussivun taustakuvat
```

---

## 🛡️ Tietoturva

- **SmartScreen päällä**: Edgen phishing- ja haittaohjelmavaroitukset ovat käytössä.
- **Moottori päivittyy itsestään**: verkkosivut ajetaan Edgen WebView2 Runtimessa, jonka Windows päivittää.
- **Osoiterivi ei valehtele**: vain neljä sisäsivua näytetään `browser://`-nimellä, muut osoitteet näkyvät sellaisenaan. `http://`-sivut saavat punaisen varoituskuvakkeen.
- **Sisäsivut suojattu**: web-sivu ei voi upottaa asetus- tai historiasivuja (clickjacking) eikä lähettää selaimelle muita komentoja kuin pikanäppäimiä.
- **`window.open`/`target=_blank`** avaa vain `http(s)`-osoitteita, ei `file://`-osoitteita.
- Kirjautumiset säilyvät uudelleenkäynnistyksen yli: `%LOCALAPPDATA%\IT-Veljekset\OmaSelain\data\WebView2` (kuten Edgessä). Poista kansio, jos haluat kirjautua kaikkialta ulos.
- Huomio: selain on uusi eikä sisällä salasananhallintaa tai laajennuksia. Google voi estää kirjautumisen upotetuissa selaimissa.

## 🔒 Yksityisyys ja tietosuoja

Oma Selain ei lähetä telemetriaa tai selaustietoja kolmansille osapuolille. Kaikki selaushistoria ja kirjanmerkit säilytetään vain ja ainoastaan käyttäjän omalla laitteella.
