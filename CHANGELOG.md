# Muutosloki / Changelog — Viima 🦀🌐

Kaikki merkittävät muutokset Viima-verkkoselaimeen dokumentoidaan tässä tiedostossa.
Formaatti noudattaa [Keep a Changelog](https://keepachangelog.com/fi/1.1.0/) -ohjeistusta ja [SemVer](https://semver.org/lang/fi/) -versiointia.

---

## [0.6.0] - 2026-11-14

### 🌟 Uutta (Added)
- **Laajennukset**: Viima lataa purettuja Chromium-laajennuksia (Manifest V3/V2) kansiosta `%APPDATA%\IT-Veljekset\Viima\config\extensions`. Asetukset → Laajennukset näyttää listan, kytkee laajennukset päälle ja pois, poistaa ne ja avaa kansion. Virheelliset kohteet siirretään `extensions-invalid`-kansioon. Laajennukset eivät ole käytössä yksityisessä ikkunassa. Opas ja esimerkki: `docs/EXTENSIONS.md`, `docs/examples/hei-viima`.
- **Asetukset sivunavigaatiolla**: Yleiset, Ulkoasu, Tietosuoja ja turvallisuus, Laajennukset, Päivitykset ja Tietoja. Osiot avautuvat osoitteella (`#extensions`, `#privacy` jne.), ja "Tallennettu"-ilmoitus vahvistaa muutokset.
- **Animaatiot**: välilehden avautuminen ja sulkeutuminen, painikkeiden painallukset, tähden täyttyminen, valikot ja asetusosioiden vaihto sekä sisäsivujen aukeaminen. Kaikki kunnioittavat `prefers-reduced-motion`-asetusta.
- **Kehitetty Suomessa** -maininta asetusten Tietoja-osiossa, aloitussivun alareunassa ja README:ssä.
- **Kehittäjädokumentaatio**: `docs/ARCHITECTURE.md` (rakenne ja mitä tallentuu minne), `docs/EXTENSIONS.md` ja `docs/CONTRIBUTING.md`.
- `VIIMA_PROFILE_DIR`: erillinen profiilikansio testaamiseen ja siirrettävään käyttöön.

### ⚡ Muutokset (Changed)
- **Aloitussivun asettelu ja taustakuva tallentuvat selaimen omaan tiedostoon** (`browser_data.json`), ei vain web-profiiliin. Ne palautuvat, vaikka WebView2-profiili poistettaisiin. Vanhat valinnat siirtyvät automaattisesti.
- Ikkunan koko tallennetaan minuutin välein, joten kaatuminen tai koneen sammutus ei hävitä sitä.

---

## [0.5.1] - 2026-11-14

### ⚡ Muutokset (Changed)
- **Puhtaampi aloitussivu**: laatikot ja reunaviivat on poistettu. Sisältö on suoraan taustakuvan päällä yhdellä vasemmalla reunalla: kello hallitsee, sää on sen vieressä, ja haku, pikalinkit ja pörssikurssit seuraavat pelkkänä tekstinä. Muokkaustila ja widgetit toimivat kuten ennen.
- Aloitussivun tallennettu asettelu nollautuu kerran uuteen oletusjärjestykseen (kello, sää, haku, linkit, kurssit).

### 🐛 Korjattu (Fixed)
- Widgetin leveysvalinta näyttää nyt oikean arvon myös, kun leveys on 7/12.

### 🧹 Siivous (Chore)
- Poistettu vanhat käynnistysskriptit (`kaynnista.*`), käyttämätön logo ja vanhan nimen ympäristömuuttujat; README päivitetty.

---

## [0.5.0] - 2026-11-14

### 🌟 Uutta (Added)
- **Muokattava aloitussivu**: uusi välilehti käyttää koko ruudun, ja Viima-merkki on poistettu. Sivu koostuu widgeteistä: haku, kello, sää, pörssikurssit ja pikalinkit kirjanmerkeistä. "Muokkaa"-painikkeella widgettejä voi lisätä, poistaa, siirtää ja leventää. Asetukset tallentuvat selaimeen.
- **Kellon ulkoasu**: digitaalinen tai analoginen, fontti, koko, väri, tasaus, 12/24 tuntia, sekunnit, päivämäärä ja tervehdys.
- **Sää** (Open-Meteo, paikkakunnan haku) ja **pörssikurssit** (Yahoo Finance, omat tunnukset, esim. `NOKIA.HE`, `^OMXH25`, `BTC-EUR`). Tiedot haetaan Rust-puolella: aloitussivun tiukka suojaus ei muutu.

### ⚡ Muutokset (Changed)
- **Oikeat faviconit**: Viima hakee ja tallentaa sivustojen ikonit itse (`favicons.json`). Kirjanmerkkipalkki, kirjanmerkkisivu, aloitussivu ja palautetut välilehdet näyttävät oikean ikonin, eikä `/favicon.ico`-arvausta enää tarvita.
- **Sisäsivut omiin välilehtiinsä**: kirjanmerkit, historia, lataukset ja asetukset (painikkeet sekä `Ctrl+H` / `Ctrl+J`) avautuvat uuteen välilehteen. Jo auki oleva sivu käytetään uudelleen.

### 🔒 Tietoturva (Security)
- Favicon-haku tavoittaa vain julkisia osoitteita: uudelleenohjaus tai DNS-huijaus ei voi ohjata hakua omaan verkkoon.

---

## [0.4.3] - 2026-11-14

### 🔒 Tietoturva (Security)
- **Allekirjoitetut päivitykset**: Viima asentaa vain päivitykset, jotka on allekirjoitettu julkaisuavaimella (ed25519). Allekirjoitus kattaa myös versionumeron, joten vanhaa versiota ei voi syöttää uutena.
- **Pikanäppäimet** luetaan suoraan näppäimistöltä: verkkosivu ei voi enää avata loputtomasti ikkunoita tai sulkea välilehtiä lähettämällä keksittyjä pikanäppäimiä.
- **Sisäsivujen suojaus (CSP)**: sisäsivuilla voi suorittaa vain Viiman omaa koodia.
- Historia ja kirjanmerkit lähetetään vain oikeille sisäsivuille.
- Kehittäjätyökalut (F12) ovat oletuksena pois päältä; ne voi kytkeä asetuksista.
- SQLite päivitetty (RUSTSEC-2026-0459).
- Julkaisuputken GitHub Actions -toiminnot lukittu tiettyihin versioihin ja riippuvuudet tarkistetaan jokaisessa julkaisussa.

---

## [0.4.2] - 2026-11-14

### ⚡ Muutokset (Changed)
- **Välilehdet**: aktiivinen välilehti sulautuu työkalupalkkiin, passiivisten välilehtien välissä ohuet erottimet ja sulkunappi näkyy vain hiiren ollessa päällä.
- **Osoiterivi** korostaa sivuston oikean osoitteen (domain kirkkaana, muu osoite himmeänä) — huijausosoitteet erottuvat helpommin.
- **Latausviiva** yläpalkin alareunassa sivun latautuessa.
- **Suojakilpi** näyttää estettyjen pyyntöjen määrän pienenä merkkinä.
- Näkyvät focus-kehykset näppäimistökäyttöön, Windowsin "vähennä liikettä" -asetuksen tuki ja Segoe UI Variable -fontti.

---

## [0.4.1] - 2026-11-13

### 🔒 Tietoturva (Security)
- Oletusselaimen linkkikäsittelijä käyttää nyt `--single-argument`-erotinta: muokattu linkki ei voi enää syöttää komentorivilippuja (esim. `--uninstall`). Asennettu Viima päivittää rekisteröinnin automaattisesti käynnistyessään.

---

## [0.4.0] - 2026-11-13

### 🌟 Uutta (Added)
- **Lataukset**: Oma lataushallinta ja `browser://downloads` (`Ctrl+J`) — edistyminen, peruutus, uudelleenyritys, avaa / näytä kansiossa. Työkalurivin latausnappi näyttää edistymisen.
- **Sivulta haku** (`Ctrl+F`, `F3`, `Shift+Enter`) osumalaskurilla.
- **Zoom** (`Ctrl +/-/0`, `Ctrl+rulla`) muistaa tason sivustokohtaisesti.
- **Komentorivin URL ja yksi instanssi**: `viima.exe https://...` tai tiedostopolku avautuu jo käynnissä olevaan ikkunaan.
- **Oletusselaimeksi rekisteröinti** asetuksista.
- **Välilehdet**: vedä ja pudota, oikean napin valikko (kiinnitä, mykistä, monista, sulje muut / oikealla olevat), kiinnitetyt välilehdet, ääni-ilmaisin ja mykistys.
- **Kirjanmerkkipalkki** (`Ctrl+Shift+B`), kirjanmerkkikansiot, `Ctrl+D` lisää kirjanmerkin.
- **Kirjanmerkkien tuonti ja vienti** HTML-muodossa (Chrome / Firefox / Edge).
- **Yksityinen ikkuna** (`Ctrl+Shift+N`): erillinen prosessi, väliaikainen profiili poistetaan sulkiessa, historia ei tallennu.
- **Mainosesto v2**: EasyList-, EasyPrivacy- ja Peter Lowe -listat (päivittyvät viikoittain), evästebannerien piilotus.
- **Sivustokohtainen sallintalista** ja estettyjen pyyntöjen erittely (mainokset / seurannat / skriptit) suojakilven valikossa.
- **Vain HTTPS -tila** varoitussivulla, kun sivusto ei tue HTTPS:ää.
- **Sivustotietojen tyhjennys**: evästeet ja tallennustila sivustokohtaisesti.
- **Snap Layouts** maksimointinapin päällä, **Jump List** tehtäväpalkissa.
- **DevTools** (`F12`, kytkin asetuksissa) ja **tulostus** (`Ctrl+P`).
- **Kieli**: suomi / englanti.
- **Uinuvat välilehdet**: 30 min käyttämättä olleet välilehdet vapauttavat muistinsa.

### ⚡ Muutokset (Changed)
- Historia siirtyi SQLite-tietokantaan (FTS5-haku). Vanha historia siirretään automaattisesti.
- Sovelluksen koodi jaettu moduuleihin.

---

## [0.3.4] - 2026-11-12

### 🌟 Uutta (Added)
- **Osoiterivin automaattinen täydennys**: Kirjoittaessa osoiteriville selain täydentää tutuimman osoitteen historiasta ja kirjanmerkeistä (esim. `yl` → `yle.fi`). Pelkkä verkkotunnus täydentyy ensin, `/`-merkin jälkeen koko polku.
- **Sivustojen faviconit välilehdissä**.
- **Estettyjen pyyntöjen laskuri**: Suojakilpi näyttää, montako mainos- ja seurantapyyntöä nykyisellä sivulla on estetty.
- **`Ctrl+1`…`Ctrl+8`** siirtyy välilehteen numerolla, **`Ctrl+9`** viimeiseen.

### ⚡ Optimoinnit (Changed)
- Taustalla olevat välilehdet siirtyvät WebView2:n matalan muistinkäytön tilaan.
- Selaintiedot tallennetaan tiiviinä JSONina, mikä keventää jokaisen sivulatauksen jälkeistä tallennusta.

---

## [0.3.3] - 2026-11-12

### 🌟 Uutta (Added)
- **Istunnon palautus**: Avoimet välilehdet, aktiivinen välilehti, ikkunan koko ja maksimointi palautuvat seuraavalla käynnistyksellä. Istunto tallentuu jatkuvasti, joten se säilyy myös kaatumisen jälkeen.
- **Kevyt palautus**: Taustalla olevat palautetut välilehdet ladataan vasta kun niihin siirrytään, joten iso istunto ei hidasta käynnistystä.
- **Pikanäppäimet**: `Ctrl+Shift+T` avaa viimeksi suljetun välilehden, `Ctrl+Tab` / `Ctrl+Shift+Tab` vaihtaa välilehteä.
- **Hiiren keskinappi** sulkee välilehden.

### ⚡ Korjaukset (Fixed)
- Historiaan tallentui usein edellisen sivun otsikko, koska otsikko saapuu vasta latauksen jälkeen. Nyt uusin merkintä päivittyy oikeaan otsikkoon.
- Ikkunan otsikko näytti sisäsivuilla "Viima" kahdesti.
- Automaattisen päivityksen palautus: jos uuden version kopiointi epäonnistui kesken, rikkinäinen tiedosto jäi paikalleen. Nyt vanha versio palautetaan aina.

---

## [0.3.2] - 2026-11-12

### ⚡ Korjaukset (Fixed)
- **Julkaisu**: v0.3.1-tagin CI-käännös kaatui uudemman clippyn varoitukseen; v0.3.2 sisältää saman korjauksen toimivana julkaisuna.
- **Mainosesto toimii nyt verkkotasolla**: Aiemmin estolista koski vain sivun päätason navigointia, joten mainosskriptit, iframet ja seurantapyynnöt latautuivat silti. Nyt jokainen pyyntö (skriptit, kuvat, XHR, iframet, workerit) suodatetaan WebView2:n `WebResourceRequested`-tapahtumassa ja estetyt palauttavat `403`.
- **Laajempi estolista**: Lisätty mm. `amazon-adsystem.com`, `google-analytics.com`, `googletagservices.com`, `adsrvr.org`, `teads.tv`, `casalemedia.com`, `indexww.com`, `3lift.com`, `media.net`, `mgid.com`, `doubleverify.com`, `adsafeprotected.com`.

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
