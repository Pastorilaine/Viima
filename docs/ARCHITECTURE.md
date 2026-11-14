# Arkkitehtuuri

Viima on Windows-selain: **Rust** (tao-ikkuna + wry) ja **WebView2** (Edgen Chromium-moottori). Selain on yksi ikkuna,
jossa on yksi *käyttöliittymä-webview* (välilehtirivi, osoiterivi) ja yksi webview jokaista välilehteä kohden.

```text
┌──────────────────────── tao-ikkuna (ei Windowsin kehystä) ───────────────────────┐
│ ui_webview   browser://ui/index.html   (assets/ui)  ← välilehdet, osoiterivi     │
│ tab-webview  https://… tai browser://newtab|settings|history|…  (yksi / välilehti)│
└───────────────────────────────────────────────────────────────────────────────────┘
        ▲  IPC: window.ipc.postMessage(JSON)               ▼  evaluate_script(JS)
                       BrowserApp (src/app) – kaikki tila yhdessä säikeessä
                                  │ AppEvent-jono (EventLoopProxy)
                                  ▼
   StorageManager (JSON) · HistoryDb (SQLite) · FaviconCache · AdBlocker · taustasäikeet (haut, päivitys)
```

## Moduulit

| Tiedosto | Tehtävä |
|---|---|
| `src/main.rs` | Sisääntulo, komentorivi (`--private`, URL), ikkuna, tapahtumasilmukka, yksi käynnissä oleva ikkuna |
| `src/app/mod.rs` | `BrowserApp`, `AppEvent`-tapahtumat, `push_state` (tila sivuille), `flush` (sulkeminen) |
| `src/app/ipc.rs` | **Luottamusraja**: JSON-viestit → `AppEvent`. Web-sivu saa lähettää vain `favicon` |
| `src/app/protocol.rs` | `browser://`-protokolla: sisäsivut upotettuna binääriin, CSP, `{{VERSION}}` |
| `src/app/tabs.rs` | Välilehtien luonti (webview-asetukset), istunnon tallennus ja palautus |
| `src/app/features.rs` | Valikot, zoom, lataukset, asetusten sivuvaikutukset, laajennuslista, ajastin |
| `src/app/remote.rs` | Faviconit, widgetien verkkohaut, aloitussivun tallennetut valinnat |
| `src/extensions.rs` | Laajennuskansiot (ks. [EXTENSIONS.md](EXTENSIONS.md)) |
| `src/storage.rs` | `browser_data.json`: kirjanmerkit, asetukset, istunto, zoom, lataukset, sisäsivujen valinnat |
| `src/history_db.rs` | Historia (SQLite) |
| `src/adblock.rs` | Mainos- ja seurantasuodatin, suodatinlistat |
| `src/updater.rs`, `update_sig.rs` | Päivitys GitHub-julkaisuista, ed25519-allekirjoituksen tarkistus |
| `src/win_*.rs`, `webview_hooks.rs` | Windows-integraatio: valikot, rekisteri, lataukset, evästeet, äänet |
| `assets/ui/` | Yläpalkki (`index.html`, `style.css`, `app.js`) |
| `assets/internal/` | Sisäsivut: `newtab`, `settings`, `history`, `bookmarks`, `downloads`, `insecure` |
| `assets/i18n.js` | Sisäsivujen käännökset (`t('avain', 'suomi', {muuttujat})`); englanti tässä, suomi HTML:ssä |

## Sisäsivut ja turvallisuus

- Sisäsivut palvellaan osoitteesta `http://browser.<sivu>/` (Windowsissa wry välittää `browser://`-osoitteen näin).
  `ipc::is_trusted_ipc_source` hyväksyy vain täsmälleen `browser.<tunnettu sivu>`.
- Sisäsivujen CSP on tiukka: `connect-src 'self'`, skriptit nonce-tunnisteella. Sivu **ei voi tehdä verkkopyyntöjä**;
  jos se tarvitsee dataa (sää, kurssit, faviconit), se pyytää IPC:llä ja Rust hakee sen kiinteästä URL-mallista
  validoiduilla parametreilla (`widgets.rs`, `favicon.rs`). Favicon-haku sallii vain julkiset IP-osoitteet (`PublicOnly`).
- Tila menee sivulle `window.onBrowserStateUpdate(state)`. Rust tarkistaa sivun `location.origin`in ennen kuin data
  ajetaan sivulle, jottei historia tai kirjanmerkit vuoda vahingossa muulle sivulle samassa välilehdessä.
- Uusi sisäsivu: lisää tiedosto `assets/internal/`, lisää nimi `storage::INTERNAL_PAGES`-listaan, `InternalPage`-enumiin
  (`app/mod.rs`) ja `protocol.rs`-tarjoiluun. Asset-muutoksen jälkeen täytyy kääntää uudelleen (`include_bytes!`).

## Mitä tallentuu ja minne

Kaikki pysyvä tieto on käyttäjän laitteella. Polut ovat Windowsissa (`VIIMA_PROFILE_DIR` korvaa ne, ks. alla).

| Tieto | Paikka | Milloin tallentuu |
|---|---|---|
| Asetukset (hakukone, teema, kieli, kirjanmerkkipalkki, adblock, HTTPS-tila, päivitykset…) | `%APPDATA%\IT-Veljekset\Viima\config\browser_data.json` | heti muutoksessa (taustakirjoittaja, väliaikainen tiedosto + uudelleennimeäminen) |
| Kirjanmerkit, lataukset, sivustokohtainen zoom | sama tiedosto | heti |
| Avoimet välilehdet, aktiivinen välilehti | sama tiedosto (`session`) | jokaisessa välilehtimuutoksessa; palautetaan käynnistyksessä |
| Ikkunan koko ja suurennettu-tila | sama tiedosto (`session`) | sulkiessa ja minuutin välein |
| Aloitussivun asettelu ja taustakuva | sama tiedosto (`page_prefs`) + sivun `localStorage` | heti; puuttuva `localStorage` täytetään tiedostosta |
| Selaushistoria | `config\history.db` (SQLite) | heti |
| Faviconit | `config\favicons.json` | minuutin välein ja sulkiessa |
| Suodatinlistat | `config\adblock_domains.txt` | viikoittain taustalla |
| Evästeet, kirjautumiset, sivustojen `localStorage`/IndexedDB | `%LOCALAPPDATA%\IT-Veljekset\Viima\data\WebView2` | WebView2 hoitaa |
| Laajennukset | `config\extensions*\` | käyttäjän kopioimat kansiot |
| Sää- ja kurssivälimuisti | sivun `localStorage` | haun jälkeen (vain nopeuttaa avausta) |

Yksityinen ikkuna ei kirjoita mitään levylle: tallennus on vain muistissa ja WebView2-profiili on väliaikainen kansio,
joka poistetaan.

Kone sammuu tai Viima kaatuu: koska muutokset kirjoitetaan heti, viimeisin tila säilyy. Ikkunan koko tallennetaan lisäksi
minuutin välein. Tiedosto kirjoitetaan väliaikaisen tiedoston kautta, joten kesken jäänyt kirjoitus ei riko sitä; viallinen
`browser_data.json` siirretään `.corrupt`-varmuuskopioksi ja selain käynnistyy oletuksilla.

## `VIIMA_PROFILE_DIR`

Absoluuttinen polku. Silloin asetukset ja historia ovat `<polku>\config`-kansiossa ja WebView2-profiili
`<polku>\WebView2`-kansiossa, ja Viima toimii omana erillisenä ilmentymänään (ei välitä linkkejä käynnissä olevalle
Viimalle). Käytä testaamiseen ja siirrettävään käyttöön.

## Julkaisu

Tagi `v*` käynnistää `.github/workflows/release.yml`: tarkistukset (audit, testit, clippy), `scripts/build_release.ps1`
(binääri, asennusohjelma, zip, `latest.json`), allekirjoitus (`VIIMA_SIGNING_KEY`) ja GitHub Release. Versio on
`Cargo.toml`issa ja `CHANGELOG.md`:ssä.
