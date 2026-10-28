# goyvoice

Latest client: compact Chats and Voice Rooms are always visible together. No server update is needed if rooms already work.

**Persistent rooms:** first-time setup requires server and client updates. Sign in as `judas`,
click **+ New** beside Voice Rooms, enter a name and creation PIN `231939`. Friends can join; the list
shows live occupancy. [Room setup and update guide](mini-voice/docs/ROOMS.md).

**[Complete installation and update guide →](mini-voice/docs/INSTALL-UPDATE.md)**

Нативний Windows-застосунок для повідомлень і голосових дзвінків до 8 учасників.
Сервер — один Rust-процес із SQLite, TLS та зашифрованим UDP; браузер не потрібен.

- [Windows-клієнт .exe](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/windows-x64/goyvoice.exe)
- [Повний Windows ZIP](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/mini-voice-windows-x64.zip)
- [Linux x86_64 сервер](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/linux-x64/mini-server)
- [Linux ARM64 сервер для Ampere](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/linux-arm64/mini-server)

**[Як запустити сервер на Oracle Always Free — українська інструкція →](mini-voice/docs/HOSTING.md)**

Підготовлено інсталятор для Ubuntu 24.04 LTS на x86_64 та ARM64: готовий сервер,
служба `systemd`, постійна база, Certbot і поновлення TLS. Він не створює хмарних
ресурсів та не потребує Oracle API-токена. Windows-клієнт залишається сумісним.

**Важливо щодо оплати:** Oracle може вимагати банківську картку для перевірки
акаунта. Цей проєкт не обходить цю вимогу. Якщо потрібен суворо варіант без
картки й Oracle її просить — ця платформа не підходить. Наявність ресурсів,
Always Free eligibility, дискові/мережеві квоти та поточні умови перевіряй у
власній консолі. Не створюй платні ресурси на припущенні, що trial їх покриє.

[Користування й збірка](mini-voice/README.md) ·
[Перевірки та обмеження](mini-voice/docs/VALIDATION.md)

Реальна VM Oracle та ACME-видача тут не запускалися. Windows GUI, фізичне аудіо
та Windows RAM/CPU залишаються неперевіреними. Публічного сервера чи готових
акаунтів у репозиторії немає.

[Audio / Profile: гучність, пристрої, аватари, Enter для надсилання та оновлення сервера](mini-voice/docs/CLIENT-UPDATE.md).
