# Inventaris van `assets/` (3 november 2026)

Wat er ligt, en wat het ons oplevert voor het protocolwerk.

| Bestand | Wat is het | Nut |
|---|---|---|
| `LiteVNA_User Guide.pdf` | Officiële LiteVNA User Guide (v1.4 firmware), 31 p. Byte-identiek (md5 `563edaab…`) aan https://nanovna.com/wp-content/uploads/2021/11/LiteVNA_User-Guide.pdf | **Hoofdbron.** Hoofdstuk 5 "Appendix – USB data interface" bevat de complete opcode- en registertabel die in `01-onderzoek-protocol.md` §3 is overgenomen, inclusief de LiteVNA-extra's (0x40 average, 0x41/0x42 power, 0x44 channel select, 0x58 tijd, 0x5C accu, 0xEE screenshot, READFIFO met NN=0 = alles). |
| `LiteVNA_User Guide-cn.pdf` | Chinese versie, 26 p., ouder | Appendix is de **oude SAA2-tabel** (alleen 0x26 = raw mode, geen 0x40..0xEE). Niet gebruiken; de Engelse is nieuwer. |
| `LiteVNA Menu Structure Map-1.2.pdf` | Menukaart van de on-device UI | Handig als referentie voor welke functies een GUI moet nabouwen (calibrate, enhanced response, TDR/transform, L/C match, SD card). |
| `liteVNA67-260713.bin`, `liteVNA64-260713.bin`, `liteVNA62-260713.bin` | Officiële firmware **v1.4.10**, build 13 juli 2026 (strings in de image: `FW Version: v1.4.10`, `Build Time: Jul 13 2026`). | Belangrijk: hiermee is **recovery via DFU mogelijk** (de Eleshop-review die zegt dat er geen .bin online staat is dus achterhaald). De image bevat USB VID/PID `04B4:0008` (Cypress CDC), wat bevestigt waar we op moeten filteren. Verder zichtbaar: SD-kaart ondersteuning, een "LiteVNA MSC"-modus (USB mass storage voor de SD-kaart), FPU-detectie, "SERIAL SPEED" en "SET TIME" menu's. Flashen laten we buiten de POC. |
| `NanoVNA-App-v1.1.218.7z` | Windows binary (alleen `NanoVNA-App.exe`, aug 2025), geen broncode | Referentie-implementatie om gedrag mee te vergelijken (onder Windows/VM). Broncode van de DiSlord-fork staat op GitHub, zie rapport 01. |
| `vna_qt_windows-18G.7z` | NanoVNA-QT (OwOComm, jan 2022), Windows build met `libxavna` | Officiële V2-software; `libxavna` is de open-source C++ library met dezelfde protocol-implementatie (https://github.com/nanovna-v2/NanoVNA-QT). |
| `NanoVNASaver.*.zip`, `nanovna-saver` | NanoVNA-Saver binaries (jan 2022) voor macOS (x86_64), Linux, Windows | Werkt op je M-serie via Rosetta. Goed om naast onze POC te leggen voor een sanity check van S11/S21-waarden. Let op: ~400 MB in totaal. |
| `litevna-projects.txt` | Je eigen linklijst | — |

## In git

Alleen `litevna-projects.txt` (de eigen linklijst) staat in git. De rest blijft lokaal in
`assets/` en wordt genegeerd (`.gitignore`):

- De zip/7z-archieven en de macOS-binary zijn samen ruim 400 MB.
- De PDF's en de firmware-images zijn klein, maar van de fabrikant: een MIT-repository
  hoort ze niet te herdistribueren. Tot 10 november 2026 stonden ze wel in git (ze zitten
  dus nog in de geschiedenis); sindsdien niet meer.

Waar ze vandaan komen, om ze opnieuw op te halen:

| Bestand | Bron |
|---|---|
| `LiteVNA_User Guide.pdf` | https://nanovna.com/wp-content/uploads/2021/11/LiteVNA_User-Guide.pdf (byte-identiek, zie boven) |
| `LiteVNA_User Guide-cn.pdf`, `LiteVNA Menu Structure Map-1.2.pdf`, `liteVNA6x-260713.bin` | de downloadpagina van de fabrikant en de bestandensectie van de LiteVNA-groep, zie de links in `assets/litevna-projects.txt` (https://nanorfe.com/nanovna-software.html, https://groups.io/g/liteVNA) |

De md5 van de User Guide staat in de tabel hierboven; van de firmware-images is het
kenmerk de versiestring `FW Version: v1.4.10`, `Build Time: Jul 13 2026` in de image.
