# Laajennukset (kehittäjän opas)

Viima ajaa **purettuja Chromium-laajennuksia**. Laajennukset ajaa WebView2 (Edgen Chromium-moottori), joten laajennus
tehdään samalla tavalla kuin Chromelle: `manifest.json`, content scriptit, service worker, popup jne. Viima ei lisää
omaa laajennusrajapintaa; se vain lataa kansion ja hallinnoi sen sisältöä Asetukset-sivulla.

> **Vaatimus:** WebView2 Runtime 120.0.2210.55 tai uudempi (Windows päivittää sen itse).
> **Tuki:** Manifest V3 (suositus) ja V2. Vain `manifest_version` 2 ja 3 hyväksytään.

## Pikaopas: ensimmäinen laajennus

1. Avaa **Asetukset → Laajennukset → Avaa kansio**. Kansio on
   `%APPDATA%\IT-Veljekset\Viima\config\extensions`.
2. Luo sinne kansio, esim. `hei-viima`, ja lisää kaksi tiedostoa.

`hei-viima/manifest.json`

```json
{
  "manifest_version": 3,
  "name": "Hei Viima",
  "version": "1.0",
  "description": "Lisää sivun alareunaan pienen viestin.",
  "content_scripts": [
    { "matches": ["https://*/*"], "js": ["hei.js"], "run_at": "document_idle" }
  ]
}
```

`hei-viima/hei.js`

```js
const note = document.createElement('div');
note.textContent = 'Hei Viimasta!';
note.style.cssText = 'position:fixed;bottom:8px;right:8px;padding:4px 8px;background:#111;color:#fff;z-index:2147483647';
document.body.appendChild(note);
```

3. **Käynnistä Viima uudelleen.** Laajennus näkyy Asetukset → Laajennukset -listassa ja toimii `https`-sivuilla.

Valmis esimerkki löytyy kansiosta [`docs/examples/hei-viima`](examples/hei-viima).

## Kansiot

| Kansio (`%APPDATA%\IT-Veljekset\Viima\config\`) | Merkitys |
|---|---|
| `extensions\<id>\manifest.json` | Käytössä, ladataan käynnistyksessä |
| `extensions-disabled\<id>\` | Kytketty pois Asetuksissa |
| `extensions-invalid\` | Kansiot ja tiedostot, jotka eivät ole laajennuksia (ei `manifest.json`-tiedostoa tai virheellinen versio). Viima siirtää ne tänne käynnistyksessä |

`<id>` on kansion nimi. Pidä se yksinkertaisena (ei `/`, `\`, `:`).

## Miten Viima käsittelee laajennuksia

- Laajennukset **ladataan kerran käynnistyksessä**. Kytkeminen päälle/pois, poistaminen ja uuden laajennuksen lisääminen
  tulevat voimaan vasta, kun Viima käynnistetään uudelleen (Asetukset näyttää siitä ilmoituksen).
- Jos yhtään laajennusta ei ole, laajennustuki ei ole päällä lainkaan: ei lisäkustannusta.
- **Yksityisessä ikkunassa laajennukset eivät ole käytössä** (kuten Chromen oletus incognito-tilassa).
- Laajennus toimii kaikissa välilehdissä jaetun profiilin kautta. Viiman omilla sisäsivuilla (`browser://…`) laajennuksia
  ei pidä tarvita; älä kohdenna niihin.

## Kehittäminen ja virheiden etsintä

- **Laajennuksen konsoli:** avaa kehittäjätyökalut (F12; kytke päälle Asetukset → Yleiset) sivulla, jossa content script
  ajaa. Content scriptin virheet näkyvät sivun konsolissa.
- **Service worker:** käynnistä Viima etädebuggausporttiin ja avaa `edge://inspect`/`chrome://inspect`:

  ```powershell
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
  .\Viima.exe
  ```

- **Eristetty testiprofiili:** älä testaa omalla profiilillasi. Aseta `VIIMA_PROFILE_DIR` (absoluuttinen polku), jolloin
  asetukset, historia, laajennukset ja WebView2-profiili ovat siinä kansiossa ja Viima ei vaihda tietoja käynnissä olevan
  Viiman kanssa:

  ```powershell
  $env:VIIMA_PROFILE_DIR = "$env:TEMP\viima-dev"
  New-Item -ItemType Directory "$env:TEMP\viima-dev\config\extensions" -Force
  cargo run
  ```

- **Päivitys kesken kehityksen:** muokkaa tiedostoja ja käynnistä Viima uudelleen. Chromium-laajennuksia ei voi ladata
  uudelleen lennossa tässä versiossa.

## Tuetut ja tuettomat rajapinnat

Tuettu on se, minkä WebView2 tukee laajennuksille. Tämä toimii: `content_scripts`, `background.service_worker`,
`chrome.storage`, `chrome.runtime`, `chrome.tabs` (rajoitetusti), `declarativeNetRequest`, `action.default_popup`.

Viiman oma käyttöliittymä (välilehtirivi, osoiterivi) ei ole laajennusten ulottuvilla, joten seuraavat eivät toimi tai
toimivat rajoitetusti: laajennuspainike työkalupalkissa, `chrome.bookmarks`, `chrome.history`, `chrome.downloads`,
`chrome.tabs.create` Viiman välilehtirivin kautta, `chrome_url_overrides` (uusi välilehti) ja omat asetussivut
(`options_page` avautuu, mutta sen avaamiseen ei ole painiketta). Viima ei ole Chrome Web Store -yhteensopiva: asenna vain
purettu kansio.

## Tietoturva

Laajennus voi lukea ja muuttaa sivuja, joihin sillä on oikeus (`matches` / `host_permissions`). Asenna vain laajennuksia,
joihin luotat, ja katso `manifest.json`-tiedostosta mitä oikeuksia ne pyytävät. Viima ei tarkista laajennuksia eikä
päivitä niitä.

## Viiman toteutus (Viiman kehittäjille)

- [`src/extensions.rs`](../src/extensions.rs): kansioiden skannaus, `prepare()` (siirtää virheelliset kohteet
  `extensions-invalid`-kansioon, koska WebView2 keskeyttää webviewin luonnin, jos kansiossa on jotain muuta kuin
  laajennuksia), `set_enabled` (siirtää kansion `extensions`- ja `extensions-disabled`-kansioiden välillä), `remove`.
  Kansion nimi validoidaan, ettei Asetukset-sivu voi viitata kansion ulkopuolelle.
- [`src/app/mod.rs`](../src/app/mod.rs): `BrowserApp::new` kutsuu `prepare()` kerran. Laajennustuki on **profiilin
  laajuinen asetus**, joten sama päätös pitää antaa jokaiselle webviewille (`with_browser_extensions_enabled`). Kansion
  polku (`with_extensions_path`) annetaan vain käyttöliittymä-webviewille, koska WebView2 lataa laajennukset profiiliin.
- IPC-viestit (vain luotetuilta sisäsivuilta): `list_extensions`, `set_extension`, `remove_extension`,
  `open_extensions_folder`. Vastaus tulee Asetuksiin funktiolla `window.onExtensions({list, started, private, folder})`.
