# Mini Voice

Нативний Windows-застосунок для повідомлень і голосових дзвінків до 8 учасників.
Сервер — один Rust-процес із SQLite, TLS та зашифрованим UDP; браузер не потрібен.

- [Windows-клієнт .exe](mini-voice/releases/windows-x64/mini-client.exe?raw=1)
- [Повний Windows ZIP](mini-voice/releases/mini-voice-windows-x64.zip?raw=1)
- [Linux x86_64 сервер](mini-voice/releases/linux-x64/mini-server?raw=1)
- [Linux ARM64 сервер для Ampere](mini-voice/releases/linux-arm64/mini-server?raw=1)

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
