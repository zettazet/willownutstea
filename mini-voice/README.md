# Mini Voice

Мінімальний клієнт Windows 10/11 x64 та власний сервер повідомлень і голосу.
Без Electron, браузера, Docker чи hosted application platform. Друзям не потрібні
Rust, Node.js, Python, Oracle-акаунт або власна VM.

## Сервер на Oracle Always Free

**[Повна інструкція: акаунт → VM → мережа → встановлення → підключення](docs/HOSTING.md)**.
Цільова система: Ubuntu 24.04 LTS, x86_64 або ARM64. Для обох архітектур є готові
Linux-бінарники; інсталятор обирає потрібний через `uname -m` і перевіряє SHA-256.

Oracle може вимагати картку для перевірки акаунта навіть для Always Free.
Якщо картка неприйнятна й реєстрація її вимагає, цей варіант не підійде.
Статус безкоштовних ресурсів і всі квоти потрібно підтвердити в консолі;
гарантії безкоштовності чи наявності вільних VM цей репозиторій не дає.

Після створення VM, DNS A-запису та правил Oracle NSG/Security List,
**у консолі Ubuntu VM**, не у Windows PowerShell:

```bash
sudo apt-get update
sudo apt-get install -y git ca-certificates
git clone --branch main --single-branch https://github.com/zettazet/willownutstea.git
cd willownutstea
# Додати правила лише для Mini Voice у брандмауері VM; існуючий SSH не видаляється.
sudo bash mini-voice/deploy/oracle/open-firewall.sh --apply
sudo bash mini-voice/deploy/oracle/install.sh \
  --domain voice.example.org --email you@example.org
```

**Заміни домен та email на свої.** TCP 80 потрібен для сертифіката/поновлення,
TCP 4433 — для клієнтів, UDP 4434 — для голосу. Домен має вказувати на публічну
IPv4 цієї VM. Потрібен свій hostname: власний домен/піддомен або безкоштовний
DDNS hostname, якщо сервіс його надає. Купівля домену не є вимогою програми.

Інсталятор встановлює службу `mini-voice` від окремого користувача, Certbot та
aвтопоновлення TLS. База лежить у `/var/lib/mini-voice/`, поза checkout, і не
видаляється при оновленні виконуваного файла. Після виходу з SSH сервер працює.

План без змін системи:

```bash
bash mini-voice/deploy/oracle/install.sh \
  --domain voice.example.org --email you@example.org --dry-run
```

## Windows-клієнт

[Завантажити mini-client.exe](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/windows-x64/mini-client.exe) ·
[повний портативний ZIP](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/mini-voice-windows-x64.zip)

1. Розпакуй ZIP та відкрий `mini-client.exe`.
2. Введи `wss://твій-домен:4433`, username та пароль → **Register**.
   Username: 3–24 ASCII літери/цифри/`_`; пароль: 10–128 UTF-8 байтів.
3. Username друга → **Add friend**; друг приймає **Accept friend**.
4. Вибери друга → **New chat**, відкрий розмову та натискай **Send**.
   Для групи вибери друзів через Ctrl-клік.
5. **Call** → інші натискають **Accept call** / **Decline**.
   Вибраних друзів можна додати через **Invite to call**. Максимум — 8 учасників.
6. У виклику: mute мікрофона, незалежний deafen та вихід. Використовуй навушники:
   echo cancellation і noise suppression не реалізовані.

Друзі вводять одну адресу сервера. На їхніх роутерах порти відкривати не потрібно.
Адреса й DPAPI-захищений токен зберігаються в `%LOCALAPPDATA%\MiniVoice\settings.json`.
Паролі на диск клієнта не записуються. Сесії діють 30 днів; один акаунт має
одне активне підключення. Онлайн **Log out** відкликає поточну сесію; офлайн-вихід
прибирає локальний токен, але не може негайно відкликати його на сервері.
Черга непідтверджених повідомлень не зберігається при закритті процесу.

## Керування Linux-сервером

```bash
sudo systemctl status mini-voice --no-pager
sudo journalctl -u mini-voice -n 80 --no-pager
sudo -u mini-voice /usr/local/bin/mini-server --check-config /etc/mini-voice/server.json
```

`--check-config` перевіряє JSON, адреси та TLS-пару без відкриття бази/портів;
не перевіряє CA-довіру, строк сертифіката, DNS чи дзвінок. Сервер приймає SIGTERM
від `systemd`, завершує операції SQLite й виконує WAL checkpoint. Перезапуск
завершує активні дзвінки; акаунти й історія зберігаються.

Інші команди: `mini-server server.json`, `mini-server --help`, `mini-server --version`.
Відносні шляхи в ручному конфігу рахуються від робочої директорії процесу.
Oracle-інсталятор використовує абсолютні шляхи. Для ручного запуску на Windows
залишається [mini-server.exe](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/windows-x64/mini-server.exe) із
`server.example.json`; Ubuntu-інсталятор для Windows не призначений.

## Для розробника

Rust 1.99.1 зафіксовано в `rust-toolchain.toml`; потрібні C/C++ toolchain і CMake.
SQLite та Opus будуються із джерел залежностей. Для CMake 4:

```bash
cd mini-voice
export CMAKE_POLICY_VERSION_MINIMUM=3.5
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p mini-server
python3 -m unittest discover -s tests/deployment -v
shellcheck deploy/oracle/*.sh
```

Нативна збірка на ARM64 використовує ті самі команди. Інсталятору можна передати
власний довірений build: `--binary /absolute/path/to/mini-server`; контроль
походження такого файла — на операторі. Для плану іншої архітектури можна
вказати `--arch arm64 --dry-run`; реальна інсталяція відхилить архітектуру,
що не відповідає VM.

Windows: Visual Studio x64 Native Tools, Desktop C++/Windows SDK, Rust MSVC,
CMake й Python 3.11+ для ліцензій. У PowerShell із `mini-voice`:

```powershell
$env:CMAKE_POLICY_VERSION_MINIMUM = '3.5'
cargo test --locked --workspace
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

## Перевірки та межі

Автоматичні Linux-тести перевіряють TLS/UDP, Opus, 8 клієнтів, історію,
перепідключення, конфігурацію й завершення SIGTERM. Окремо перевіряються
інсталятор та unit-файл. Це не доказ живого розгортання в Oracle чи ACME-renewal.
Windows GUI/фізичне аудіо та цілі <50 MiB idle / <100 MiB у дзвінку невиміряні.
Сервер читає повідомлення й голос: end-to-end encryption не заявлено.

[Архітектура](docs/ARCHITECTURE.md) · [Перевірки](docs/VALIDATION.md) ·
[Вимірювання](docs/BENCHMARKS.md)

## Робота у фоні та діагностика

Хрестик, Alt+F4 або згортання ховають клієнт у системний трей біля годинника.
Повідомлення, підключення та поточний дзвінок продовжують працювати; мікрофон
залишається у попередньому стані. Наведи курсор на іконку: підказка показує
Background/Open, стан підключення та активний дзвінок/вимкнений мікрофон.
Іконка може бути у прихованих значках під стрілкою біля годинника.

Клік на іконку відкриває вікно. Права кнопка → Open Mini Voice або
Exit (end calls). Кнопка Exit app у вікні також повністю завершує клієнт.
Якщо трей недоступний, вікно залишається доступним через панель завдань.
Режим у фоні не означає автозапуск після перезавантаження Windows і не працює,
коли ПК вимкнений або спить.

Якщо клієнт завершується з помилкою, перевір журнал:
%LOCALAPPDATA%\MiniVoice\client.log (можна вставити у Win+R).
Журнал містить етапи запуску та місце panic без тексту повідомлень, паролів
і токенів; попередній журнал — client.previous.log. При внутрішній помилці
показується діалог із розташуванням журналу. Збої Windows до запуску програми
можуть не потрапити в журнал.

Оновлення: **Audio / Profile** — гучність, пристрої, аватар і Show Avatars.
Enter надсилає, Shift+Enter додає новий рядок. Для аватарів онови також сервер.
Деталі: [Audio / Profile та оновлення сервера](docs/CLIENT-UPDATE.md).
