# Развёртывание ClickHouse OLAP Engine на Ubuntu VPS

Цель: ClickHouse + Rust engine на одном VPS, доступ из Excel на Windows по HTTPS.

---

## 1. Требования к серверу

| Параметр | Минимум | Рекомендуется |
|----------|---------|---------------|
| ОС | Ubuntu 22.04 LTS | Ubuntu 24.04 LTS |
| CPU | 2 vCPU | 4+ vCPU |
| RAM | 4 GB | 8+ GB (ClickHouse жадный к памяти) |
| Диск | 20 GB SSD | 50+ GB SSD |
| Порты | 80, 443, 22 | то же |

---

## 2. Первоначальная настройка сервера

```bash
# Подключиться к серверу
ssh root@YOUR_VPS_IP

# Обновить систему
apt update && apt upgrade -y

# Создать системного пользователя для движка
useradd -m -s /bin/bash engine
usermod -aG sudo engine

# (Опционально) настроить SSH-ключи для engine
# su - engine && ssh-keygen -t ed25519
```

---

## 3. Установка ClickHouse

```bash
# Официальный репозиторий ClickHouse
apt install -y apt-transport-https ca-certificates curl gnupg
curl -fsSL https://packages.clickhouse.com/rpm/lts/repodata/repomd.xml.key \
  | gpg --dearmor -o /usr/share/keyrings/clickhouse-keyring.gpg

echo "deb [signed-by=/usr/share/keyrings/clickhouse-keyring.gpg] \
  https://packages.clickhouse.com/deb stable main" \
  > /etc/apt/sources.list.d/clickhouse.list

apt update && apt install -y clickhouse-server clickhouse-client

# Запустить и добавить в автозагрузку
systemctl enable --now clickhouse-server
systemctl status clickhouse-server
```

### Настройка ClickHouse для сетевого доступа от движка

По умолчанию ClickHouse слушает только `127.0.0.1` — это правильно, движок на том же сервере.

```bash
# Проверить что CH слушает локально
ss -tlnp | grep 8123
# должно быть: 127.0.0.1:8123

# Создать пользователя и БД для приложения
clickhouse-client --query "
  CREATE DATABASE IF NOT EXISTS olap;
  CREATE USER IF NOT EXISTS olap_user IDENTIFIED BY 'STRONG_PASSWORD_HERE';
  GRANT ALL ON olap.* TO olap_user;
"
```

### Создать таблицу и загрузить данные

```bash
# Скопировать seed-скрипт на сервер или выполнить вручную:
clickhouse-client --query "
CREATE TABLE IF NOT EXISTS olap.sales (
    order_id         UInt64,
    customer_id      UInt64,
    order_date       Date,
    region           LowCardinality(String),
    country          LowCardinality(String),
    product_category LowCardinality(String),
    amount           Decimal(18, 2),
    qty              UInt32
) ENGINE = MergeTree()
ORDER BY (region, order_date, order_id)
"

# Проверить
clickhouse-client --query "SELECT count() FROM olap.sales"
```

---

## 4. Установка Rust и сборка движка

```bash
# Переключиться на пользователя engine
su - engine

# Установить Rust (rustup)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# Проверить
rustc --version  # должно быть stable 1.80+
```

### Способ A: сборка на сервере из исходников

```bash
# Клонировать репозиторий
git clone https://github.com/ivanshamaev/clickhouse-olap.git
cd clickhouse-olap/engine

# Собрать release-бинарник (занимает ~3–5 минут)
cargo build --release

# Бинарник готов:
ls -lh target/release/engine
```

### Способ B: скачать готовый бинарник из GitHub Release

```bash
# Скачать релиз (заменить версию на актуальную)
VERSION="0.1.0"
curl -L "https://github.com/ivanshamaev/clickhouse-olap/releases/download/v${VERSION}/clickhouse-olap-engine-v${VERSION}-x86_64-unknown-linux-gnu.tar.gz" \
  | tar -xz

sudo mv engine /usr/local/bin/clickhouse-olap-engine
sudo chmod +x /usr/local/bin/clickhouse-olap-engine
```

---

## 5. Конфигурация движка

```bash
# Создать директорию для конфига
sudo mkdir -p /etc/clickhouse-olap/models
sudo chown -R engine:engine /etc/clickhouse-olap

# Создать конфиг (если сборка из исходников — скопировать пример)
# Иначе создать /etc/clickhouse-olap/config.toml вручную:
```

**`/etc/clickhouse-olap/config.toml`:**
```toml
[server]
# Engine слушает локально; nginx проксирует снаружи
bind = "127.0.0.1:3000"

[clickhouse]
url      = "http://127.0.0.1:8123"
database = "olap"
username = "olap_user"
password = "STRONG_PASSWORD_HERE"
query_timeout_secs    = 30
max_preaggregate_rows = 500000

[models]
path = "/etc/clickhouse-olap/models"

[cache]
capacity = 1000
ttl_secs = 300

[limits]
max_cells  = 500000
max_groups = 200000
```

**`/etc/clickhouse-olap/models/sales_model.toml`:** — скопировать из репозитория:

```bash
cp /home/engine/clickhouse-olap/engine/config/models/sales_model.toml \
   /etc/clickhouse-olap/models/
```

---

## 6. systemd-сервис для движка

```bash
sudo tee /etc/systemd/system/clickhouse-olap-engine.service > /dev/null << 'EOF'
[Unit]
Description=ClickHouse OLAP Engine
After=network.target clickhouse-server.service
Requires=clickhouse-server.service

[Service]
Type=simple
User=engine
Group=engine
WorkingDirectory=/home/engine
ExecStart=/home/engine/clickhouse-olap/engine/target/release/engine \
    --config /etc/clickhouse-olap/config.toml
Restart=on-failure
RestartSec=5s
StandardOutput=journal
StandardError=journal
SyslogIdentifier=clickhouse-olap-engine

# Ограничения безопасности
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/tmp

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now clickhouse-olap-engine
sudo systemctl status clickhouse-olap-engine

# Проверить логи
journalctl -u clickhouse-olap-engine -f
```

---

## 7. HTTPS через nginx + Let's Encrypt

Office.js **требует** HTTPS даже для локальных подключений. Нужен домен.

```bash
# Установить nginx и certbot
sudo apt install -y nginx certbot python3-certbot-nginx

# Получить SSL-сертификат (нужен домен, указывающий на IP сервера)
sudo certbot --nginx -d engine.yourdomain.com

# Настроить nginx как reverse proxy
sudo tee /etc/nginx/sites-available/clickhouse-olap << 'EOF'
server {
    listen 80;
    server_name engine.yourdomain.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name engine.yourdomain.com;

    ssl_certificate     /etc/letsencrypt/live/engine.yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/engine.yourdomain.com/privkey.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    # CORS — нужен для Office.js
    add_header Access-Control-Allow-Origin  "https://ivanshamaev.github.io" always;
    add_header Access-Control-Allow-Methods "GET, POST, OPTIONS"             always;
    add_header Access-Control-Allow-Headers "Content-Type"                   always;

    # Preflight
    if ($request_method = OPTIONS) {
        return 204;
    }

    location /v1/ {
        proxy_pass         http://127.0.0.1:3000;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Real-IP         $remote_addr;
        proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
        proxy_send_timeout 60s;
    }
}
EOF

sudo ln -sf /etc/nginx/sites-available/clickhouse-olap \
            /etc/nginx/sites-enabled/clickhouse-olap
sudo nginx -t && sudo systemctl reload nginx
```

---

## 8. Firewall

```bash
sudo ufw allow 22/tcp    # SSH
sudo ufw allow 80/tcp    # HTTP (redirect → HTTPS)
sudo ufw allow 443/tcp   # HTTPS (nginx → engine)
sudo ufw enable
sudo ufw status

# ClickHouse и движок НЕ открываем напрямую в интернет:
# 8123 (ClickHouse HTTP) — только 127.0.0.1
# 3000 (engine)          — только 127.0.0.1
```

---

## 9. Проверка работы

### С VPS (curl внутри)

```bash
# Health
curl http://127.0.0.1:3000/v1/health
# → {"status":"ok"}

# Список моделей
curl http://127.0.0.1:3000/v1/models
```

### Снаружи (с Windows-машины, через браузер или curl)

```bash
# Health через nginx/HTTPS
curl https://engine.yourdomain.com/v1/health
# → {"status":"ok"}
```

### Из надстройки Excel

1. Открыть Excel → надстройка ClickHouse OLAP
2. Settings → Engine URL: `https://engine.yourdomain.com`
3. Нажать **Test Connection** → должно показать "✓ Connected"

---

## 10. Обновление движка

```bash
su - engine
cd clickhouse-olap
git pull origin main
cd engine
cargo build --release

sudo systemctl restart clickhouse-olap-engine
sudo systemctl status clickhouse-olap-engine
```

---

## 11. Логи и мониторинг

```bash
# Логи движка (реалтайм)
journalctl -u clickhouse-olap-engine -f

# Логи nginx
tail -f /var/log/nginx/access.log
tail -f /var/log/nginx/error.log

# Логи ClickHouse
tail -f /var/log/clickhouse-server/clickhouse-server.log

# Проверить потребление ресурсов
htop
# или
systemctl status clickhouse-olap-engine clickhouse-server
```

---

## 12. Безопасность (чеклист)

- [ ] ClickHouse слушает только `127.0.0.1:8123`, не открыт в интернет
- [ ] Движок слушает только `127.0.0.1:3000`, не открыт в интернет
- [ ] UFW включён: открыты только 22, 80, 443
- [ ] SSL-сертификат Let's Encrypt установлен и обновляется автоматически
- [ ] Отдельный пользователь ClickHouse с минимальными правами (`GRANT ON olap.*`)
- [ ] Пользователь `engine` без sudo-привилегий в production
- [ ] `NoNewPrivileges=yes` и `ProtectSystem=strict` в systemd-сервисе
- [ ] CORS в nginx ограничен нужными доменами (не `*`)

---

## Быстрый старт (команды одним блоком)

```bash
# На свежем Ubuntu 22.04 от root:
apt update && apt upgrade -y
apt install -y curl git nginx ufw

# ClickHouse
curl -fsSL https://packages.clickhouse.com/rpm/lts/repodata/repomd.xml.key \
  | gpg --dearmor -o /usr/share/keyrings/clickhouse-keyring.gpg
echo "deb [signed-by=/usr/share/keyrings/clickhouse-keyring.gpg] \
  https://packages.clickhouse.com/deb stable main" \
  > /etc/apt/sources.list.d/clickhouse.list
apt update && apt install -y clickhouse-server
systemctl enable --now clickhouse-server

# Пользователь engine + Rust
useradd -m -s /bin/bash engine
su - engine -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
su - engine -c 'source ~/.cargo/env && git clone https://github.com/ivanshamaev/clickhouse-olap.git'
su - engine -c 'source ~/.cargo/env && cd clickhouse-olap/engine && cargo build --release'

# Конфиг и запуск (подставить реальный пароль и домен)
mkdir -p /etc/clickhouse-olap/models
# ... (настроить config.toml и модели как в разделе 5)
# ... (настроить systemd как в разделе 6)
# ... (настроить nginx как в разделе 7)
```
