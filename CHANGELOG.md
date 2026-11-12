# Muutosloki / Changelog — Viima 🦀🌐

Kaikki merkittävät muutokset Viima-verkkoselaimeen dokumentoidaan tässä tiedostossa.
Formaatti noudattaa [Keep a Changelog](https://keepachangelog.com/fi/1.1.0/) -ohjeistusta ja [SemVer](https://semver.org/lang/fi/) -versiointia.

---

## [0.3.0] - 2026-11-12

### 🌟 Uutta (Added)
- **Uusi brändi ja identiteetti**: Selaimen viralliseksi nimeksi valittu **Viima** (kuvastaa nopeutta, keveyttä ja pohjoismaisuutta).
- **Viima-logo ja Windows-sovellusikoni**:
  - Korkearesoluutioinen revontuliteemainen app-logo ja favicon (`assets/branding/viima-logo.png`).
  - Monikokoinen Windows `.ico` -kuvake (16, 32, 48, 256 px) upotettu suoraan Windows SDK:n `rc.exe`-resursseihin sekä selaimeen että asennusohjelmaan.
- **Natiivi Windows-asennusohjelma (`Viima-Setup.exe`)**:
  - Yksitiedostoinen Rust-asennusohjelma ilman ulkoisia asennustyökaluja (Inno Setup / NSIS ei tarvita).
  - Asentaa selaimen käyttäjäkohtaisesti polkuun `%LOCALAPPDATA%\IT-Veljekset\Viima` ilman järjestelmänvalvojan oikeuksia.
  - Luo Käynnistä-valikon ja valinnaisen työpöydän `Viima.lnk`-pikakuvakkeet.
  - Rekisteröi Windowsin *Sovellukset ja ominaisuudet* -poistomerkinnän (`Viima v0.3.0`, koko, kuvake ja hiljainen poistokomento).
  - Tukee `--silent`, `--no-desktop` ja `--launch` -komentorivikytkimiä.
  - Tarkistaa Microsoft Edge WebView2 Runtime -asennuksen ja tarjoaa latauslinkin tarvittaessa.
- **Automaattinen päivitysjärjestelmä (Auto-Updater)**:
  - GitHub Releases -pohjainen päivitystarkistus (`latest.json` ja Releases API -tuki).
  - SHA-256 -tarkistussumman varmennus ennen asennusta ja turvallinen HTTPS-lataus.
  - Taustalataus ja automaattinen binäärin vaihto uudelleenkäynnistyksen yhteydessä.
  - Käyttöliittymässä visuaalinen `#btnUpdateApp`-ilmoituspainike ja kattava päivityshallinta asetuksissa (`browser://settings`).
  - Tukee komentorivilippuja `--apply-update`, `--cleanup-update` ja `--uninstall`.
- **Aloitussivun taustakuvat ja kustomointi**:
  - 3 generoitua taustakuvaa (`aurora-dark.jpg`, `glass-waves.jpg`, `nordic-light.jpg`) ja dynaaminen teemaan mukautuva taustakuvavalitsin.
- **Kirjanmerkit aloitussivulla**:
  - Pikalinkit korvattu käyttäjän tallentamilla kirjanmerkeillä puhtaassa 4-sarakkeisessa suosikkiruudukossa.
  - Tunnistaa suosituimmat verkkosivustot (*Iltalehti*, *Yle*, *YouTube*, *GitHub*, *DuckDuckGo*, *Rust*, *Wikipedia*, *Sää*, *ChatGPT*, *Google*) ja näyttää niiden autenttiset brändilogot.

### ⚡ Korjaukset & optimoinnit (Fixed & Changed)
- **Välilehtipalkin asettelu**: Korjattu CSS-syntaksivirhe, jonka vuoksi toinen välilehti asettui pystysuunnassa edellisen alle ja valui osoiterivin päälle. Lisätty ehdoton `flex-wrap: nowrap` ja säiliön `overflow: hidden`.
- **Muistinkäyttö & haku**: Optimoidut bang-haut, nopea O(log n) adblock-verkkotunnusten binaarihaku sekä suoraviivaiset välimuistipuskurit.
- **Kansiorakenne & nimeäminen**: Siivottu projektin juuri ja keskitetty kaikki valmiit julkaisut kansioihin `releases/latest/` ja `releases/v0.3.0/`. Binäärit ja asennuspaketit nimetty selaimen mukaisesti muotoon `Viima.exe` ja `Viima-Setup.exe`.

---

## [0.2.1] - 2026-11-12

### 🌟 Uutta (Added)
- Taustakuvien vaihtomahdollisuus aloitussivulla (`browser://newtab`).
- Vaalean ja tumman tilan automaattinen taustakuvasynkronointi.

### ⚡ Korjaukset (Fixed)
- Välilehtien otsikoiden tekstikatkaisu ja hakurivin fokusointiongelmat.

---

## [0.2.0] - 2026-11-09

### 🌟 Uutta (Added)
- Sisäänrakennettu mainos- ja seurantasuodatin (AdBlock).
- Tuki bang-oikopoluille (`!g`, `!d`, `!w`, `!yt`, `!gh`, `!yle`).
- Pysyvä paikallinen tallennus kirjanmerkeille ja selaushistorialle (`browser_data.json`).
- Sisäiset sivut: `browser://newtab`, `browser://bookmarks`, `browser://history`, `browser://settings`.

---

## [0.1.0] - 2026-11-08

### 🌟 Ensimmäinen versio (Initial Release)
- Rust + Wry / Tao -pohjainen suomalainen työpöytäselain.
- Monivälilehtituki, Chromium/WebView2-pohjainen nopea verkkosivumoottori ja suomenkielinen käyttöliittymä.
