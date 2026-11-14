# Osallistuminen ja kehitys

Viima on suomalainen projekti. Ongelmat ja muutosehdotukset voi kirjoittaa suomeksi tai englanniksi.

## Aloitus

```powershell
git clone https://github.com/Pastorilaine/Viima
cd Viima
cargo run
```

Tarvitset **Rustin** (1.85+, edition 2024) ja Windows 10/11:n. WebView2 Runtime tulee Windowsin mukana.

**Testaa omalla erillisellä profiililla**, ettei kehitysversio kirjoita oikean selaimesi asetuksiin tai istuntoon:

```powershell
$env:VIIMA_PROFILE_DIR = "$env:TEMP\viima-dev"
cargo run
```

Yksityinen ikkuna (`cargo run -- --private`) ei kirjoita levylle ollenkaan.

## Tarkistukset ennen muutoksen jättämistä

```powershell
cargo test
cargo clippy --all-targets -- -D warnings
```

Samat ajetaan julkaisussa (`.github/workflows/release.yml`). Jos käynnissä on raskas peli, aja Cargo matalalla
prioriteetilla: `Start-Process cargo -ArgumentList build -PassThru | % { $_.PriorityClass = 'BelowNormal' }`.

## Käyttöliittymän muuttaminen

- Käyttöliittymä (`assets/ui`) ja sisäsivut (`assets/internal`) upotetaan binääriin (`include_bytes!`). **Muutoksen
  jälkeen käännä uudelleen**, muuten näet vanhan version.
- Sisäsivujen CSP kieltää verkkopyynnöt ja ulkoiset fontit. Käytä Windowsin omia fontteja (`Bahnschrift`, `Cascadia Mono`).
- Uudet tekstit: suomi kirjoitetaan HTML:ään (`data-i18n="avain"`), englanti `assets/i18n.js`-tiedostoon samalla avaimella.
- Liike kunnioittaa `prefers-reduced-motion`-asetusta. Animaatio vastaa käyttäjän tekoon (välilehti avautuu, valikko
  aukeaa, painike painuu), ei pyöri itsekseen.
- Visuaalinen linja: ei lasimorfismia, pehmeitä varjoja eikä korttiruudukkoja. Rivit erotetaan ohuella viivalla.

## Asetuksen lisääminen

1. `storage.rs`: kenttä `BrowserSettings`-rakenteeseen (`#[serde(default)]`, jotta vanhat tiedostot latautuvat) ja
   tarvittaessa `set_setting`-haaraan validointi. Jos vanhat arvot pitää muuttaa, nosta `settings_version` ja lisää `migrate()`.
2. `app/features.rs` (`set_setting`): sivuvaikutus, jos asetus muuttaa jotain heti.
3. `assets/internal/settings.html`: lisää `.row` oikean osion (`<section class="panel">`) sisään. Kytkimelle riittää
   `<input type="checkbox" data-setting="kentän_nimi">`: tila ja tallennus (`set_setting`) hoituvat jo. Uusi osio on
   yksi `.nav-item`-linkki ja yksi `<section class="panel" id="…">`.
4. Käännökset `assets/i18n.js`. Lisää testi `storage.rs`:n testeihin.

## Uusi IPC-viesti sisäsivulta

1. `ipc.rs`: lisää haara `parse`-funktioon. Oletuksena viesti on **vain luotetuille sisäsivuille**; web-sivu ei saa
   koskaan lähettää muuta kuin `favicon`ia. Lisää testi, joka varmistaa ettei web-sivu pääse viestillä läpi.
2. `app/mod.rs`: lisää `AppEvent`-variantti ja käsittely `handle_event`issä.
3. Validoi kaikki sivulta tuleva (pituudet, sallitut avaimet, polut). Sivua ei pidä uskoa.

## Laajennukset

Ks. [EXTENSIONS.md](EXTENSIONS.md).

## Commit-viestit ja versiot

Lyhyt, kuvaava otsikko (`fix: …`, `feat: …`). Käyttäjälle näkyvät muutokset kirjataan `CHANGELOG.md`-tiedostoon
suomeksi. Julkaisu: nosta versio `Cargo.toml`issa, päivitä `CHANGELOG.md`, aja `cargo build` (päivittää `Cargo.lock`in),
luo tagi `vX.Y.Z` ja työnnä se. Julkaisuputki tekee loput.
