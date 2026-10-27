# Mini Voice для Windows

1. Розпакуй усю папку та відкрий mini-client.exe.
2. Введи адресу свого сервера: wss://твій-домен:4433.
3. Введи username і пароль → Register.
4. Додай друга: Add friend → інший користувач натискає Accept friend.
5. Вибери друга → New chat; для дзвінка Call → Accept call.

Друзям не потрібні Oracle-акаунти чи власні сервери. Сервер має бути налаштований
один раз оператором: https://github.com/zettazet/willownutstea/blob/main/mini-voice/docs/HOSTING.md
Для Oracle Ubuntu потрібен Linux-сервер, а не Windows mini-server.exe.

mini-server.exe у цій папці — альтернативний ручний Windows-сервер. Потрібні
server.json (копія server.example.json) та власна коректна TLS-пара; запуск
із PowerShell у папці: .\mini-server.exe .\server.json.

Використовуй навушники. Echo cancellation не реалізовано. Збірки unsigned;
Windows GUI, фізичне аудіо та RAM/CPU тут не перевірялися. Дивись docs/VALIDATION.md.
Під час поширення зберігай LICENSE, THIRD-PARTY-NOTICES.txt і compiler-notices.
