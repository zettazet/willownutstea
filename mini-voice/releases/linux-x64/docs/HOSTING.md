# Oracle Always Free: встановлення goyvoice

Ціль: **Ubuntu 24.04 LTS** на Oracle Compute VM, x86_64 або ARM64.
Сервер працює постійно як служба; домашній ПК тримати ввімкненим не потрібно.
Ця інструкція не створює Oracle-акаунт/VM і не використовує хмарних API-токенів.

## 1. Спочатку перевір реєстрацію та вартість

Oracle може вимагати банківську картку для перевірки особи/акаунта та тимчасове
блокування коштів. **Якщо потрібен варіант суворо без картки й Oracle її просить,
зупини реєстрацію: встановлення сервера не обходить вимоги провайдера.**

У своєму акаунті перевір:

- ресурси мають саме **Always Free eligible**, а не лише оплату з trial-кредиту;
- обрана shape, RAM/OCPU, boot volume і мережевий трафік входять у доступні квоти;
- потрібний публічний IPv4 та інші додані опції не створюють витрат;
- поточні правила малоактивних VM, строків/лімітів і доступності у твоєму регіоні.

Не переходь на Pay As You Go заради отримання відсутньої безкоштовної shape,
якщо твоя вимога — не витрачати кошти. Budget alerts не слід вважати гарантованим
автоматичним обмеженням оплати. Відсутність вільної Always Free VM — причина
відкласти встановлення, а не створити платну машину.

Офіційні сторінки:
[Oracle Free](https://www.oracle.com/cloud/free/),
[Always Free resources](https://docs.oracle.com/en-us/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm).
Доступ до цих сторінок із нашого build-середовища повертав 403, тому тут немає
непідтверджених фіксованих сум кредиту чи обіцянок кількості безкоштовних ядер/GB.

## 2. Створи Compute VM

В Oracle Cloud Console: **Compute → Instances → Create instance**.

- Image: **Ubuntu 24.04 LTS**; не Oracle Linux для цього інсталятора.
- Shape: лише доступна тобі Always Free eligible.
  Приклади назв, якщо їх пропонує поточний офер: `VM.Standard.A1.Flex` — ARM64,
  `VM.Standard.E2.1.Micro` — x86_64. Самої назви недостатньо: перевір позначку/квоти.
- Розмір: невелика VM, наприклад 1 CPU та 1–2 GiB RAM, якщо це дозволено квотою.
  Це початковий вибір, не навантажувальний benchmark.
- Boot volume: достатній мінімум для Ubuntu в межах безкоштовної квоти.
  Не додавай необов'язкові платні диски/бекапи.
- Networking: public subnet, public IPv4, маршрут `0.0.0.0/0` через Internet Gateway.
- SSH: додай публічний ключ або збережи приватний ключ, якщо його генерує Oracle.

Приватний ключ залишається на твоєму ПК, не в репозиторії й не в чаті.
Для Ubuntu звичайний користувач — **ubuntu**. Запиши публічну IPv4 машини.
Інсталятор автоматично обере потрібний готовий бінарник, Rust на VM не потрібен.

## 3. Налаштуй Oracle ingress і брандмауер Ubuntu

Є **два незалежні рівні**, які повинні дозволяти підключення:

1. Oracle **Network Security Group**, прикріплена до VNIC, або **Security List**
   відповідної підмережі.
2. Брандмауер у самій Ubuntu; в OCI-образах може бути фінальне правило REJECT,
   навіть якщо правила в консолі вже додані.

У NSG/Security List додай **stateful ingress**:

| Протокол | Destination port | Source CIDR | Призначення |
|---|---:|---|---|
| TCP | 22 | твоя IP/32 | SSH, зберегти чинний доступ |
| TCP | 80 | `0.0.0.0/0` | видача й поновлення TLS через HTTP-01 |
| TCP | 4433 | `0.0.0.0/0` | вхід, повідомлення, TLS WebSocket |
| UDP | 4434 | `0.0.0.0/0` | голос |

**Source port range залиш All**, заповнюй саме destination port.
NSG має бути прикріплена до цієї VM. Залиш вихідний доступ для DNS, apt та ACME;
простий початковий варіант — стандартний allow-all egress.
SSH не закривай до перевірки альтернативного console-доступу.

Пізніше підготовлений `open-firewall.sh --apply` додасть лише 80/TCP, 4433/TCP,
4434/UDP у активний UFW або перед REJECT у iptables, зберігаючи інші правила.
Він не скидає firewall, не переписує SSH і не змінює Oracle NSG/Security List.
Для iptables ставить `iptables-persistent` і зберігає поточний ruleset.

## 4. DNS та SSH

Створи DNS **A**-запис, наприклад `voice.your-domain.net` → IPv4 цієї VM.
Можна використати наявний домен/піддомен або безкоштовний DDNS hostname, якщо
провайдер його надає. Платний домен не є вимогою застосунку.
CDN/proxy вимкни (у Cloudflare — **DNS only**). AAAA не додавай, поки сервер
слухає лише IPv4. Якщо IP зміниться, онови DNS.

У **PowerShell на своєму Windows-ПК**, замінивши файл ключа та IP:

```powershell
ssh -i "$env:USERPROFILE\Downloads\oracle.key" ubuntu@203.0.113.10
```

Це приклади, не готовий ключ чи адреса сервера. Якщо потрібний SSH-ключ уже
налаштовано стандартно, достатньо `ssh ubuntu@ТВОЯ_IP`.
**Усі наступні bash-команди виконуються в Ubuntu VM**, не в Windows PowerShell.

## 5. Клонуй main і запусти встановлення

```bash
sudo apt-get update
sudo apt-get install -y git ca-certificates
git clone --branch main --single-branch https://github.com/zettazet/willownutstea.git
cd willownutstea
getent ahostsv4 voice.your-domain.net
```

Остання команда має показати публічну IPv4 цієї VM. Виправ DNS до продовження.
Для приватного репозиторію потрібен read-only deploy key; не вставляй приватні
токени в URL Git і не коміть ключі.

Подивитися план без змін системи:

```bash
bash mini-voice/deploy/oracle/open-firewall.sh --dry-run
bash mini-voice/deploy/oracle/install.sh \
  --domain voice.your-domain.net --email your-email@example.net --dry-run
```

Потім установити, **замінивши домен і email на свої**:

```bash
sudo bash mini-voice/deploy/oracle/open-firewall.sh --apply
sudo bash mini-voice/deploy/oracle/install.sh \
  --domain voice.your-domain.net --email your-email@example.net
```

Інсталятор перевіряє checksum бінарника, ставить Certbot/SQLite CLI/OpenSSL,
створює окремого користувача `mini-voice`, постійні каталоги, службу `systemd`,
TLS та поновлення сертифіката. API-токен Oracle не потрібен.
TCP 80 на VM має бути вільний для Certbot standalone. Якщо там уже web-сервер,
не зупиняй його навмання: налаштуй ACME webroot/DNS-01 вручну або використай
окрему VM. Не вимикай TLS і не встановлюй вигадані сертифікати для обходу помилок.

Сервер після інсталяції не працює як root. База поза checkout, оновлення її
не видаляє. Повторний запуск із тим самим доменом/стандартним конфігом підтримано;
змінений або невідомий конфіг інсталятор не перезаписує.
Власний довірений build передається через `--binary /absolute/path/to/mini-server`;
за походження й архітектуру custom build відповідає оператор.

## 6. Перевір службу, TLS і клієнт

```bash
sudo systemctl status mini-voice --no-pager
sudo journalctl -u mini-voice -n 80 --no-pager
sudo -u mini-voice /usr/local/bin/mini-server --check-config /etc/mini-voice/server.json
```

Очікується `active (running)`. Перевірка конфігу не відкриває базу/порти та
не доводить доступність із інтернету чи дійсність домену. Для зовнішнього TLS
на машині з OpenSSL:

```bash
openssl s_client -connect voice.your-domain.net:4433 \
  -servername voice.your-domain.net -verify_hostname voice.your-domain.net \
  -verify_return_error </dev/null
```

У Windows-клієнті кожен вводить:

```text
wss://voice.your-domain.net:4433
```

Друзі відкривають [mini-client.exe](../releases/windows-x64/mini-client.exe?raw=1)
або [портативний ZIP](../releases/mini-voice-windows-x64.zip?raw=1), реєструють
окремі акаунти (**Register**), додають друзів (**Add friend / Accept friend**),
створюють **New chat** та виклики **Call / Accept call**. Друзям не потрібен Oracle.

Перевір удвох із різних мереж: повідомлення, перезапуск клієнта з історією,
реальний звук, mute/deafen та короткий обрив інтернету. Потім — груповий дзвінок.
При блокуванні UDP клієнт має перейти на голос через TLS, із можливою більшою
затримкою. Хмарний сервер не виправляє обриви останньої ділянки інтернету учасника.

## 7. Керування й TLS renewal

| Шлях | Що зберігається |
|---|---|
| `/usr/local/bin/mini-server` | виконуваний файл |
| `/etc/mini-voice/server.json` | конфіг, абсолютні шляхи |
| `/etc/mini-voice/tls/` | сертифікат і приватний ключ із правами 0640 |
| `/etc/letsencrypt/` | Certbot і renewal-конфіг |
| `/var/lib/mini-voice/mini-voice.sqlite` | акаунти, сесії, друзі, повідомлення |
| `/etc/systemd/system/mini-voice.service` | автозапуск і sandbox служби |

```bash
sudo systemctl restart mini-voice
sudo systemctl stop mini-voice
sudo systemctl start mini-voice
sudo journalctl -u mini-voice -f
sudo systemctl list-timers certbot.timer
sudo certbot renew --dry-run
```

SIGTERM від `systemd` обробляється штатно, SQLite робить checkpoint. Перезапуск
сервера завершує поточні виклики, але зберігає акаунти/історію.
`certbot.timer` автоматично перевіряє поновлення. Після успішної видачі deploy hook
копіює ключ/chain, перевіряє їх від імені сервісного користувача й перезапускає
сервер. TCP 80 має залишатися відкритим для HTTP-01. Звичайний `renew --dry-run`
перевіряє ACME, але не є доказом реального deploy hook/поновлення.

Служба має `MemoryMax=256M`: це захисний ліміт, не виміряне споживання RAM.
Для власних лімітів використовуй `sudo systemctl edit mini-voice`, а не редагуй
керований unit напряму. Перевищення ліміту може завершити процес і виклики.

## 8. Оновлення та резервні копії

У тому самому checkout на VM:

```bash
git pull --ff-only origin main
sudo bash mini-voice/deploy/oracle/install.sh \
  --domain voice.your-domain.net --email your-email@example.net
```

Служба перезапуститься. База залишається в `/var/lib/mini-voice/`; попередній
бінарник копіюється в `/usr/local/bin/mini-server.previous`. Спершу збережи backup.
Для узгодженої онлайн-копії SQLite з WAL:

```bash
sudo install -d -m 0700 /var/backups/mini-voice
sudo sqlite3 /var/lib/mini-voice/mini-voice.sqlite \
  ".backup '/var/backups/mini-voice/backup-$(date -u +%Y%m%dT%H%M%SZ).sqlite'"
```

Каталог backup доступний лише root. Для завантаження спочатку зроби окрему
копію для користувача `ubuntu` (заміни `ІМЯ.sqlite` на фактичне ім'я):

```bash
sudo install -o ubuntu -g ubuntu -m 0600 \
  /var/backups/mini-voice/ІМЯ.sqlite /home/ubuntu/ІМЯ.sqlite
```

Потім завантаж `/home/ubuntu/ІМЯ.sqlite` через SCP/SFTP своїм SSH-ключем;
зберігай її приватно й прибери тимчасову копію з home після завантаження.
Не копіюй лише `.sqlite` під час запису
та не вважай backup на тій самій VM захистом від її видалення/вилучення.
Для відновлення зупини службу, збережи старі файли та віднови копію з власником
`mini-voice` і правами 0600. Не видаляй VM/boot volume до отримання копії даних.
Немає гарантії безстрокового збереження Oracle малоактивної безкоштовної VM;
перевір чинні правила, не імітуй навантаження заради їх обходу.

## Якщо не працює

- **Картка обов'язкова:** застосунок цього не обходить; обирай інший хостинг.
- **Out of capacity / немає Always Free:** не переходь на платну shape, якщо бюджет 0.
- **Exec format error:** невірна архітектура; `uname -m`, x64 ≠ ARM64.
- **ACME challenge failed:** перевір A/відсутність зайвого AAAA, TCP 80 на обох
  рівнях firewall, маршрут через Internet Gateway і відсутність іншого web-сервера.
- **Клієнт не підключається:** `wss://...:4433`, DNS only, NSG і OS firewall,
  сертифікат для того самого імені. Не вводь лише IP із доменним сертифікатом.
- **Повідомлення є, голос нестабільний:** UDP 4434, навушники, пристрої, TLS fallback.
- **Служба впала:** `sudo journalctl -u mini-voice -n 100 --no-pager`.
- **Після renewal/оновлення завершився дзвінок:** перезапуск завершує transient calls;
  створи новий виклик.

Реальну Oracle VM, її NSG, ACME та поновлення не перевірено в build-середовищі.
[VALIDATION.md](VALIDATION.md) описує виконані перевірки й решту acceptance-тестів.

Оновлення клієнта Audio / Profile додає серверні аватари. Для наявної VM дивись
[інструкцію оновлення без скидання акаунтів](CLIENT-UPDATE.md#updating-your-existing-oracle-server).
