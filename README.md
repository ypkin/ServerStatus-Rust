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

按第 3 节写好 `data/config.toml`（`data` 里只手写这一个文件；`stats.json`、`traffic.json`、`admin.json`、`ping_history.json` 由服务端自动生成，缺失按空加载，**不要手建空 JSON 文件**，空文件不是合法 JSON）。Docker 下 `config.toml` 里必须加 `workspace = "/data"`（否则服务端默认写容器内 `/opt/ServerStatus`，不进 `./data` 卷，容器重建会丢数据）。

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

无需等待构建，拉取镜像即可启动。启动后打开 `http://<服务端IP>:8080`，能看到面板即服务端已起。镜像是 amd64 + arm64 双架构，x86 与 ARM 机器自动选对版本。`latest` 只在 docker.yml 构建成功后更新（触发方式见第 9 节；master 有新提交不等于 latest 已更新）。

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

# workspace 指定数据目录：Docker 下填 "/data"，裸二进制下留空即默认 /opt/ServerStatus
# workspace = "/data"

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
- **数据目录**：`workspace` 指定数据文件（stats / traffic / admin / 探针历史）写到哪里，默认 `/opt/ServerStatus`。Docker 下必须填 `"/data"`（与 compose 的 working_dir / 卷一致），否则数据写容器内、重建会丢；裸二进制下留空即默认，与 WorkingDirectory 一致。
- **鉴权密钥**：`jwt_secret` 留空则每次启动随机生成，重启后需重新登录 `/admin`；想长期有效就填一个固定值。
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

## 5. 后台操作（https://<你的域名>/admin，全部点鼠标）

登录：账号默认填 `admin`（`config.toml` 里 `admin_user` 不是 `admin` 就改成实际账号），密码填 `admin_pass`。登录 token 只存 sessionStorage，关掉页面就失效；服务端重启后也要重登（`jwt_secret` 默认每次启动随机生成，见第 3 节）。顶部导航：节点 / 流量 / 探针 / 推送 / 设置；「回主面板」回前台，「退出登录」清 token。

### 节点：添加、编辑、删除、排序

点「添加节点」，填：

- 节点名 *（唯一，如 `cheap3`）、密码 *（节点上报用的密码；编辑时留空表示不改）、显示名（默认同节点名）
- 服务器分组 Tag（前台按 Tag 分组，空为默认组）
- 地区、类型（如 `kvm`）、排序（留空自动）
- 流量限额 GiB（`0`=不限量）、计费模式（`sum` 双向 / `up` 仅上行 / `down` 仅下行 / `max` 取大者）、重置日（1–31，默认 1）
- 开始日期、结束日期（只显示填过的值，绝不推导；留空就一直空）
- 年费 + 货币（`$` / `¥` / `£` / `€` / 自定义符号如 `HK$`）、带宽 + 单位（Mbps / Gbps / Tbps）
- 备注（仅后台可见）、公开备注（显示在主面板卡片上，行里绿笔可直接改）

点「保存」。添加成功后表单下方会显示这台节点的一键安装命令（已包含节点名与密码，在新机器上以 root 执行，客户端自动下载、安装并上报）；已添加的节点点安装命令同样能看到（一键命令 + 直接运行的客户端命令两条）。注意：目前一键命令装的是上游客户端（没有本分支的 ICMP 探针与服务端下发探针）；要完整功能按第 6 节手动装 fork 客户端。

编辑节点：点行里的编辑改上面这些字段；密码留空表示不改。删除节点：点删除（会弹窗确认；删除后节点立即从面板下线，但节点上的客户端不会自己停——它会鉴权失败，要到节点上另行停掉服务）。批量删除 / 批量修改分组在表上方（先勾选节点再点；批量改分组会弹窗让输新 Tag，留空归默认组）。

排序：拖动行首 ☰ 换位，点「保存排序」才生效；分组筛选（非全部）时不能排序，先切回全部。

节点表：IP 两行（v4 / v6）各带复制按钮（复制完整地址；v6 显示截断约 19 字符、悬停看完整）；协议列在节点列之前；分组药丸在标题旁；限额 `0` 显示不限量。

### 分组：UI 里只有 Tag，模板在 config 里

后台没有单独的分组管理页，分组分两层：

- 前台分组 = 节点的 Tag（上面添加/批量改的地方填，自由文本，空归默认组）。
- 分组模板 = `config.toml` 的 `hosts_group`（模板字段地区 / 类型 / 通知 / 标签 / 流量限额 / 计费模式 / 开始日期 / 年费 / 带宽会被成员节点继承；后台单台覆盖优先于模板；删组不踢人，成员保留）。模板改完要重启服务端才生效；后台单台改动立即生效，不用重启。
- 客户端用 `-g 分组名 -p 分组密码` 接入即在该分组下自动建节点，新成员继承模板。

### 流量：看数、改设置、修正

流量视图列出每台节点的今日 / 本月 / 总计 / 已用（按计费模式折算）/ 限额。

- 改限额 / 模式 / 重置日：走节点编辑表单（上面那些字段）。
- 修正：填一个已用值（GiB），按节点计费模式对齐到设定值（`up` 只改出方向、入方向保留；`down` 反之；`max` 取大；`sum` 相加），并盖当前周期戳，之后采集往上加；总计自动补到不小于本月。修正后已用精确等于填的值。

### 探针：后台统一管，不用逐台改命令行

探针视图：先勾「启用三网 ping」并保存启用状态；IPv4 / IPv6 各三格（默认联通 / 电信 / 移动，名称可改），每格填名称 + 地址（纯域名或 IP，不要带 `http://`；端口不再需要，带了会被忽略；IPv6 加不加方括号都行；地址空着就是没设置，不测也不显示；IPv6 留空表示该运营商不测 v6）。点「保存探针设置」，客户端下次上报自动生效，不用逐台改命令行。

探针总览：点「刷新总览」看 v4/v6 实测值（主面板只显示 v4 三个值，v6 值请在这里看；`—` 表示未探测：未配 v6 地址、节点无 v6，或暂未上报）。

### 推送：掉线 PushPlus

推送视图：勾「启用 PushPlus」；填 PushPlus token（留空表示不改；token 只存服务端，不会在本页回显）、标题（默认 `ServerStatus`）、渠道（默认 `wechat`）、掉线模板（留空用默认文案：别名 (节点名) 掉线）、恢复模板。点「保存 PushPlus 设置」。掉线/恢复模板为空时用默认文案推送。

### 设置

设置视图只有当前登录账号、主面板链接与退出登录；节点、流量、探针、推送的设置分别在各自视图里改。

后台改动持久化在服务端 `admin.json`（0600），不改写 `config.toml`；不要直接改磁盘上的 `admin.json`（服务端内存里有一份，外部改完下次后台保存会把旧值写回去；要改先停服务、改完再启动）。

## 6. 客户端部署（预编译包下载安装；不要用面板的 /i 一键脚本）

`/i` 下发的一键脚本从上游 Release 下载客户端（仅 systemd 服务），没有本分支的 ICMP 探针与服务端下发探针；要完整功能必须手动装 musl 版。**预编译包已发布**，推荐直接下载（见下）；只有改源码才需要自己构建（见 6.1）。

**下载预编译包（推荐）：**

```bash
mkdir -p /opt/ServerStatus
curl -L -o /opt/ServerStatus/stat_client https://github.com/ypkin/ServerStatus-Rust/releases/latest/download/stat_client
chmod +x /opt/ServerStatus/stat_client
```

固定版本下载把 `latest` 换成标签名，如 `client-v1`：`https://github.com/ypkin/ServerStatus-Rust/releases/download/client-v1/stat_client`。下载后按下面 6.2（systemd）或 6.3（OpenRC）配服务。

### 6.1 自行构建（改源码才需要；构建机上做，不必是节点本身）

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

更新：下载新版 Release 覆盖（或重新构建后覆盖），再重启（systemd：`systemctl restart stat_client`；OpenRC：`rc-service stat_client restart`）。

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

## 9. 维护者：发布新版

### 客户端新版

打 `client-v*` 标签并推（如 `git tag client-v2 && git push origin client-v2`，标签名每次换新的），或 Actions → Release client → Run workflow 填新 tag；工作流自动编 musl 静态版并挂到 Release 上。§6 的 latest 链接自动跟进新版；固定版本示例需手动改标签名。打 `client-v*` 标签不会触发上游遗留的 release.yml（它只认 `v*`），不用管它。

### 服务端新版镜像

Actions → Build and Push to GHCR → Run workflow（在 master 上手动触发）；master push 不自动跑，`port/traffic-admin-ping` push 才自动跑（README / .gitignore 改动不触发）。跑绿后 ghcr 的 `latest` 更新，用户侧 `docker compose pull && docker compose up -d` 即可。
