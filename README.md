# ✨ Rust 版 ServerStatus 云探针（fork）

> 本仓库是 [zdz/ServerStatus-Rust](https://github.com/zdz/ServerStatus-Rust) 的 fork，主线为 `master`。本文档是本分支的部署指南：从一台空机器开始，把服务端与客户端完整跑起来。
>
> 本分支在上游基础上的主要改动：后台管理 `/admin`（节点 / 分组 / 套餐 / 流量修正 / 三网探针 / PushPlus）、服务端流量累计与计费口径（sum / up / down / max）、ICMP 三网探针（服务端下发、客户端执行）、探针历史持久化。
>
> 部署时服务端与客户端均用本分支的构建产物，**不要混用上游 Release 二进制**：上游客户端没有 ICMP 探针与服务端下发探针，上游服务端没有后台、流量累计与探针历史。前端定制随服务端二进制一起发布（overlay 经 RustEmbed 编进二进制），无需单独部署前端。

## 1. 服务端：Docker 方式（推荐）

预编译镜像已发布（amd64 + arm64 双架构，匿名拉取无需登录），Docker 部署推荐直接 pull，不用本地构建。

```bash
git clone https://github.com/ypkin/ServerStatus-Rust.git
cd ServerStatus-Rust
mkdir -p data
```

按第 3 节写好 `data/config.toml`（`data` 里只手写这一个文件；`stats.json`、`traffic.json`、`admin.json`、`ping_history.json` 由服务端自动生成，缺失按空加载，**不要手建空 JSON 文件**，空文件不是合法 JSON）。

在仓库根目录新建 `docker-compose.yml`：

```yaml
services:
  ssr-server:
    image: ghcr.io/ypkin/serverstatus-rust:latest
    container_name: ssr-server
    restart: unless-stopped
    working_dir: /data
    command: ["-c", "/data/config.toml"]
    ports:
      - "8080:8080"   # 面板与 /report
      - "9394:9394"   # gRPC；前面有 nginx 且客户端都走 https /report 时可不映射公网
    volumes:
      - ./data:/data
    environment:
      - TZ=Asia/Shanghai   # 改成你自己的时区
```

```bash
docker compose up -d
```

无需等待构建，拉取镜像即可启动。启动后打开 `http://<服务端IP>:8080`，能看到面板即服务端已起。镜像是 amd64 + arm64 双架构，x86 与 ARM 机器自动选对版本。`latest` 只在 docker.yml 构建成功后更新（目前需手动触发，master 有新提交不等于 latest 已更新）。

日常命令：

```bash
docker compose logs -f ssr-server   # 看日志
docker compose restart ssr-server   # 重启
docker compose pull && docker compose up -d   # 更新服务端
docker compose down                 # 停止（数据在 ./data 里，不会丢）
cp -r data data.bak-$(date +%F)     # 备份数据目录（config + 统计 + 后台 + 探针历史都在里面）
```

**本地构建（只有改了源码才需要，平时不用）：**把上面 `image:` 一行换成下面三行，再 `docker compose up -d --build`：

```yaml
    build:
      context: .
      dockerfile: Dockerfile.ghcr
    image: ssr-server:latest
```

首次构建约 10–20 分钟（在镜像里编服务端及依赖；Dockerfile 未做依赖分层缓存，改代码重编也是全量）。完整 release 构建建议 2GB 以上内存；小内存机器先加 swap，或在别的机器上编好再把二进制拷过来。

## 2. 服务端：裸二进制方式（备选）

```bash
git clone https://github.com/ypkin/ServerStatus-Rust.git
cd ServerStatus-Rust
touch server/build.rs   # 否则 banner 里的提交号可能缓存成旧值
cargo build --release -p stat_server
```

产物在 `target/release/stat_server`。把它和仓库里的 `systemd/stat_server.service` 配合使用。文件里的路径默认就是 `/opt/ServerStatus`，按下面布局放就不用改；换了目录才改 `WorkingDirectory` 与 `ExecStart` 两行：

```ini
[Service]
WorkingDirectory=/opt/ServerStatus
ExecStart=/opt/ServerStatus/stat_server -c /opt/ServerStatus/config.toml
Restart=on-failure
```

```bash
mkdir -p /opt/ServerStatus
cp target/release/stat_server /opt/ServerStatus/
cp systemd/stat_server.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now stat_server
```

日常命令：

```bash
journalctl -u stat_server -f -n 100   # 看日志
systemctl restart stat_server          # 重启
systemctl status stat_server           # 看状态
```

更新服务端：重新构建后把新二进制拷过去覆盖，再 `systemctl restart stat_server`。备份：整个 `/opt/ServerStatus/` 目录拷一份（`config.toml` 与数据文件都在里面；WorkingDirectory 指到哪里，数据文件就写到哪里）。

Alpine / OpenRC 机器没有 systemd：参照第 6 节客户端的 OpenRC 写法，把命令换成服务端的启动命令（`stat_server -c /opt/ServerStatus/config.toml`），`rc-update add stat_server default && rc-service stat_server start`。

## 3. 服务端配置 config.toml（详解）

仓库根目录的 `config.toml` 就是带注释的完整示例；下面是能跑起来的最小版。`admin_user` / `admin_pass` 必须填，留空会随机生成且启动日志不打印，等于把自己锁在 `/admin` 外面：

```toml
grpc_addr = "0.0.0.0:9394"
http_addr = "0.0.0.0:8080"
offline_threshold = 30

admin_user = "<管理员账号>"
admin_pass = "<强密码>"

# 单机模式：name 唯一，password 是客户端 -p 要用的密码
hosts = [
  {name = "node1", password = "<节点密码>", alias = "节点 1", location = "cn", type = "kvm"},
]

# 或者分组模式（二选一）：客户端用 -g <分组名> -p <分组密码> 接入即自动建节点
# hosts_group = [
#   {gid = "g1", password = "<分组密码>", location = "cn", type = "kvm"},
# ]

# 套餐字段（节点与分组都可加；后台也能改，改动即时生效不用重启）：
# traffic_limit 按字节，0 = 不限量（不要填 9999，旧约定已退役）；
# traffic_mode = "sum" | "up" | "down" | "max"（up 只计出方向、down 只计入方向、max 取大、空归 sum）；
# monthstart = 每月重置日 1–31（留空为自然月）；
# plan_start_date = "2026-01-15"；annual_fee = 年费；bandwidth_mbps = 带宽
# 例：{name = "node1", password = "…", traffic_limit = 1099511627776, traffic_mode = "sum", monthstart = 1, plan_start_date = "2026-01-15", annual_fee = 15.99, bandwidth_mbps = 1000}

[ping]
enabled = true
cu_name = "联通"
ct_name = "电信"
cm_name = "移动"
# 探针地址：IP 或域名加端口，v6 加方括号；端口会被自动剥离（ICMP 不依赖端口），旧地址无需重填
# 空 = 该路未配置，跳过不探（不是故障）；电信没有探针地址就留空
cu_addr = "<联通探针地址>"
ct_addr = ""
cm_addr = "<移动探针地址>"
cu_addr_v6 = ""
ct_addr_v6 = ""
cm_addr_v6 = ""

[pushplus]
enabled = false
# token 在后台填自己的，不要用别人的；channel 一般填 wechat
token = ""
title = "ServerStatus"
channel = "wechat"
```

字段说明：

- **监听**：`grpc_addr` 是 gRPC 端口（默认 9394，客户端直连用）；`http_addr` 是面板与 `/report` 端口（默认 8080）。公网只需暴露反代的 80/443，8080/9394 保持本机监听即可；客户端都走 https `/report` 时 9394 可不映射公网。
- **下线判定**：`offline_threshold` 是多少秒无上报判下线（默认 30）。
- **管理员**：`admin_user` / `admin_pass` 是 `/admin` 的登录账号，必须填。后台改动持久化在服务端 `admin.json`（0600），不改写 `config.toml`。
- **单机与分组**：`hosts` 是单机模式（每台一个 `name`，唯一，`password` 对应客户端 `-p`）；`hosts_group` 是分组模式（客户端用 `-g <分组名> -p <分组密码>` 接入即自动建节点）。二选一，不要混写同一台。`alias` 是展示名，`location` 是国家/地区（国旗用），`type` 是虚拟化类型。单机还可加 `labels`（自定义标签）、`notify = false`（单独关掉线告警）、`disabled = true`（单机禁用）。
- **有效设置优先级**：后台填过的值优先，其次是分组模板的套餐默认值，最后才是 `config.toml`。后台改动即时生效，不用重启服务端。
- **流量口径**：主面板「总流量」为网卡开机累计（`network_in / network_out`）；「月流量」为本月累计（`month_rx / month_tx`）；详情行只显示本月三项：已用 · 剩余 · 每月限制。计费模式 `sum | up | down | max`（up 只计出方向、down 只计入方向、max 取大、空或未知归 sum）；重置日 1–31 号（留空为自然月）。vnstat 节点保持原路，老客户端走旧基线。
- **探针**：`[ping]` 的 `enabled` 是总开关；`cu / ct / cm` 分别是联通 / 电信 / 移动，`_name` 是显示名，`_addr` 与 `_addr_v6` 是 v4 / v6 探针地址。探针为 ICMP echo，只测到主机的往返，不依赖目标端口，配置里的端口自动剥离。探针历史由服务端持久化（分钟桶与小时桶均值），只能从部署起攒，不编造。后台改探针地址后客户端下次上报自动生效，不用逐台改命令行。
- **推送**：`[pushplus]` 的 `token` 在后台填自己的（只存服务端、接口不回显），`channel` 一般填 `wechat`。掉线 / 恢复有默认文案。

另注意两点：`server_url` 不用填，服务端按请求 Host 自动生成——所以反代必须透传 Host，否则面板的 `/i` 一键命令会生成内网地址；`admin.json` 是后台的持久化文件，**外部直接改它必须先停服务**，改完再启动，否则下次后台保存会把内存旧值整文件写回。

## 4. 反代 + TLS（nginx 示例）

```nginx
server {
    listen 80;
    server_name <你的域名>;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name <你的域名>;
    ssl_certificate /etc/nginx/certs/<你的域名>_cert.pem;
    ssl_certificate_key /etc/nginx/certs/<你的域名>_key.pem;
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $http_host;   # 关键：必须透传 Host，否则 /i 命令生成内网地址
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

证书用 certbot 申请（HTTP-01 需 80 端口可达，DNS-01 适合已有域名解析），路径按自己环境改。改完配置先 `nginx -t` 检查，再 `nginx -s reload` 生效。公网只需暴露 80/443；8080/9394 保持本机监听即可。

## 5. 后台初始化（/admin）

打开 `https://<你的域名>/admin`，用 `config.toml` 的 admin 账号登录。后台改动即时生效，不用重启服务端：

1. **节点**：建节点（或等客户端自动注册）。节点表支持增删改、拖动排序（拖完点保存）、国旗图片、IP 两行显示 + 复制按钮、年费货币（¥ / $ / £ / € / 自定义）、带宽单位（Mbps / Gbps / Tbps，存 Mbps、自适应显示）、开始 / 结束日期、公开备注行内改、一键命令重新获取。
2. **分组**：后台可建分组模板（地区、类型、流量限额、套餐默认值）；客户端用 `-g 分组名 -p 分组密码` 接入即自动建节点，新成员继承模板的套餐；删除分组不踢出已注册成员。
3. **套餐**：节点与分组均可设限额、模式、重置日、开始日期、年费、带宽。限额填 `0` = 不限量，不要填 `9999`。**结束日期只填已知值，绝不推导，空永远是空**。
4. **流量修正**：表单填的是「已用」（按计费模式折算的量，不是入 + 出之和）；修正后已用精确等于填的值，之后采集往上加，总量只升不降且恒 ≥ 本月。
5. **三网探针**：填联通 / 电信 / 移动 × v4 / v6 的探针地址与开关（格式见第 3 节）；后台改完客户端下次上报自动生效，不用逐台改命令行。空配置的列显示 — 属正常未配置，不是故障。
6. **掉线推送 PushPlus**：填**自己的** token，开关与模板按需改（token 只存服务端，接口不回显）。掉线 / 恢复有默认文案，模板为空时自定义 / 流量告警不发。

## 6. 客户端部署（musl 手动安装；不要用面板的 /i 一键脚本）

`/i` 下发的一键脚本从上游 Release 下载客户端（仅 systemd 服务），没有本分支的 ICMP 探针与服务端下发探针；要完整功能必须手动装 musl 版。**客户端没有预编译包**，必须自己构建（或从已装好的节点把二进制拷过来）。

### 6.1 构建（构建机上做，不必是节点本身）

```bash
rustup target add x86_64-unknown-linux-musl
# Debian/Ubuntu：apt install -y musl-tools   # 提供 musl-gcc
cargo build --release --target x86_64-unknown-linux-musl -p stat_client
```

产物在 `target/x86_64-unknown-linux-musl/release/stat_client`。Alpine 构建机自带 musl，直接编就是静态版，不用加 `--target`。动态版不要用：在 glibc 较低的机器上起不来。

拷到节点并给执行权限：

```bash
mkdir -p /opt/ServerStatus
cp target/x86_64-unknown-linux-musl/release/stat_client /opt/ServerStatus/stat_client
chmod +x /opt/ServerStatus/stat_client
```

### 6.2 systemd（Debian / Ubuntu / CentOS 等）

参照仓库 `systemd/stat_client.service`，只改 `ExecStart` 一行（`-a` 是服务端地址，`-u` 是节点名或 `-g` 是分组名，`-p` 是对应密码）：

```ini
[Service]
User=root
WorkingDirectory=/opt/ServerStatus
ExecStart=/opt/ServerStatus/stat_client -a "https://<你的域名>/report" -u <节点名> -p <节点密码>
Restart=on-failure
```

分组接入把 `-u <节点名>` 换成 `-g <分组名>`。无域名时 `-a` 可写 `http://<服务端IP>:8080/report`。然后：

```bash
cp systemd/stat_client.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now stat_client
```

日常命令：

```bash
systemctl status stat_client          # 看状态
journalctl -u stat_client -f -n 100  # 看日志
systemctl restart stat_client         # 重启（改参数 / 换二进制后用）
```

### 6.3 OpenRC（Alpine 等）

在节点上新建 `/etc/init.d/stat_client`（模板，参数照抄上面 `ExecStart` 引号里的内容）：

```sh
#!/sbin/openrc-run
description="ServerStatus-Rust Client"
command="/opt/ServerStatus/stat_client"
command_args="-a \"https://<你的域名>/report\" -u <节点名> -p <节点密码>"
command_background="yes"
pidfile="/run/stat_client.pid"
```

```bash
chmod +x /etc/init.d/stat_client
rc-update add stat_client default
rc-service stat_client start
```

日常命令：`rc-service stat_client status` 看状态，`rc-service stat_client restart` 重启。

### 6.4 参数与行为

- `-a`：服务端 `/report` 地址。走域名用 `https://<你的域名>/report`，无域名用 `http://<服务端IP>:8080/report`。
- `-u <节点名>`：单机模式，节点名须与 `config.toml` 的 `hosts[].name` 或后台建的节点一致。
- `-g <分组名>`：分组模式，分组名须与 `config.toml` 的 `hosts_group[].gid` 或后台建的分组一致，接入即自动建节点。
- `-p <密码>`：对应节点或分组的密码，须与 `config.toml` 或后台一致，否则注册不上。
- `--iface`：多网卡 / 隧道机器可用它只上报指定网卡，避免隧道流量双计（具体写法见客户端 `--help`）。
- **客户端须以 root 运行**：ICMP 需 DGRAM / RAW 权限；非 root 需配 `ping_group_range` 或 `CAP_NET_RAW`。

客户端持续向 `/report` 上报系统数据（30 秒无上报判下线），同时从服务端拉取探针配置并执行 ICMP 探测：DGRAM 优先、RAW 兜底，打不开 socket 记丢包，**绝不回落 TCP**；探针地址里的端口自动剥离，旧地址无需重填。

### 6.5 更新与卸载

更新：重新构建后把新二进制拷过去覆盖，再重启（systemd：`systemctl restart stat_client`；OpenRC：`rc-service stat_client restart`）。

卸载：先停服务再删文件（systemd：`systemctl disable --now stat_client` 后删 `/etc/systemd/system/stat_client.service` 与 `/opt/ServerStatus/stat_client`；OpenRC：`rc-service stat_client stop && rc-update del stat_client default` 后删 `/etc/init.d/stat_client` 与二进制）。

注册不上先查三处：节点名 / 分组名与密码是否与 `config.toml` 或后台一致，`-a` 地址是否可达（浏览器打开 `https://<你的域名>/report` 应有响应），客户端日志有无持续报错。

## 7. 验收清单

服务端：

- 面板 `https://<你的域名>` 能打开，节点在线（30 秒无上报判下线）。
- `/admin` 能登录，后台改动即时生效。
- 反代 Host 透传正确：面板的 `/i` 一键命令生成的是公网域名地址，不是内网地址。

客户端：

- 服务端状态无持续报错（systemd：`systemctl status stat_client`；OpenRC：`rc-service stat_client status`）。
- 面板上节点在线，月流量随上报增长。
- 探针格有数值；空配置的列显示 — 属正常未配置，不是故障。

浏览器看到旧版先硬刷新：overlay 每页只加载一次，旧标签页要重载才拿新版。

## 8. 常见坑

- 限额填 `0` = 不限量，不要填 `9999`。
- 结束日期不推导；`admin.json` 外改先停服务。
- 数据目录不要手建空 JSON 文件。
- 小内存机器不要本地构建（易 OOM），直接 pull 镜像；非要本地构建才加 swap 或异地构建再拷二进制。
- 旧 overlay / bundle 文件不要删：旧缓存的 HTML 还引用它们，删了会 404。改前端（overlay）必须重构建服务端 + 重部署；overlay 用路径版本号（新文件、旧文件保留），部署后公网按字节比对带版本号的文件，容器本机正确不等于浏览器拿到正确版。
- 探针历史只能从部署起攒，过去数据不编造；延迟图档位 1h / 6h / 24h / 7d。
- 客户端注册不上：先查节点名 / 分组名与密码是否与 `config.toml` 或后台一致，`-a` 地址是否可达。
- 客户端探针没数据：先查探针地址是否填了（空列是正常未配置），再查 ICMP 是否被墙、客户端是否以 root 运行。
- 客户端流量双计：隧道机器用 `--iface` 只上报指定网卡。
- 客户端动态版起不来：必须用 musl 静态版，动态版在 glibc 较低的机器上起不来。
- 不要混用上游 Release 二进制：上游客户端没有 ICMP 探针与服务端下发探针，上游服务端没有后台、流量累计与探针历史。
