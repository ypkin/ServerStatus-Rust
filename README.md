<p align="center">
  <a href="https://github.com/zdz/ServerStatus-Rust">
    <h1 align="center">✨ Rust 版 ServerStatus 云探针</h1>
  </a>
</p>

<div align="center">
    <p>
        <a href="https://github.com/zdz/ServerStatus-Rust/actions/workflows/release.yml">
            <img src="https://github.com/zdz/ServerStatus-Rust/actions/workflows/release.yml/badge.svg" alt="Release"></a>
        <a href="https://github.com/zdz/ServerStatus-Rust/issues">
            <img src="https://img.shields.io/github/issues/zdz/ServerStatus-Rust"
                  alt="GitHub issues">
        </a>
        <a href="https://github.com/zdz/ServerStatus-Rust/discussions">
            <img src="https://img.shields.io/github/discussions/zdz/ServerStatus-Rust"
                  alt="GitHub Discussions">
        </a>
        <a href="https://github.com/zdz/ServerStatus-Rust/releases">
            <img src="https://img.shields.io/github/v/release/zdz/ServerStatus-Rust"
                  alt="GitHub release (latest SemVer)">
        </a>
        <a href="https://github.com/zdz/ServerStatus-Rust/releases">
            <img src="https://img.shields.io/github/downloads/zdz/ServerStatus-Rust/total" alt="GitHub all releases">
        </a>
    </p>
</div>

<img width="1317" alt="image" src="https://user-images.githubusercontent.com/152173/206825541-6eaeb856-0c03-479a-b07e-006b60b41c02.png">
<img width="1436" alt="image" src="https://user-images.githubusercontent.com/152173/165958225-25fc8fda-5798-42f8-bac5-72d778c0bab5.png">



> 本仓库是 [zdz/ServerStatus-Rust](https://github.com/zdz/ServerStatus-Rust) 的 fork，工作分支为 `port/traffic-admin-ping`。上游文档见下文；本分支新增功能见文末「Fork 新增功能」。

<h2>Table of Contents</h2>

- [1. 介绍](#1-介绍)
  - [🍀 主题](#-主题)
- [2. 安装部署](#2-安装部署)
  - [2.1 快速体验](#21-快速体验)
  - [2.2 快速部署](#22-快速部署)
  - [2.3 服务管理脚本](#23-服务管理脚本)
  - [2.4 Railway 部署](#24-railway-部署)
  - [2.5 Heroku 部署](#25-heroku-部署)
- [3. 服务端说明](#3-服务端说明)
  - [3.1 配置文件 `config.toml`](#31-配置文件-configtoml)
  - [3.2 服务端运行](#32-服务端运行)
- [4. 客户端说明](#4-客户端说明)
  - [4.1 Rust 版 Client](#41-rust-版-client)
  - [4.2 Python 版 Client](#42-python-版-client)
- [5. 开启 `vnstat` 支持](#5-开启-vnstat-支持)
- [6. FAQ](#6-faq)
- [7. 相关项目](#7-相关项目)
- [8. 最后](#8-最后)

## 1. 介绍
  `ServerStatus` 威力加强版，保持轻量和简单部署，增加以下主要特性：

- 使用 `rust` 完全重写 `server`、`client`，单个执行文件部署
- 多系统支持 `Linux`、`MacOS`、`Windows`、`Android`、`Raspberry Pi`
- 支持上下线和简单自定义规则告警 (`telegram`、`wechat`、`email`、`webhook`)
- 支持 `http` 协议上报，方便部署到各免费容器服务和配合 `cf` 等优化上报链路
- 支持 `cloudflare tunnels` 和 `mTLS` 部署
- 支持主机分组动态注册，简化配置
- 支持 `vnstat` 统计月流量，重启不丢流量数据
- 支持 `railway` 快速部署
- 支持 `systemd` 开机自启
- 其它功能，如 🗺️  见 [wiki](https://github.com/zdz/ServerStatus-Rust/wiki)

演示：[ssr.rs](https://ssr.rs) | [cn dns](https://ck.ssr.rs)
|
下载：[Releases](https://github.com/zdz/ServerStatus-Rust/releases)
|
[Changelog](https://github.com/zdz/ServerStatus-Rust/releases)
|
反馈：[Discussions](https://github.com/zdz/ServerStatus-Rust/discussions)

📚 完整文档迁移至 [doc.ssr.rs](https://doc.ssr.rs)

📚 保姆级教程 [Google](https://www.google.com/search?q=%22serverstatus-rust%22)
|
[Bing](https://www.bing.com/search?q=%22serverstatus-rust%22)

### 🍀 主题

如果你觉得你创造/修改的主题还不错，欢迎分享/PR，前端单独部署方法参考 [#37](https://github.com/zdz/ServerStatus-Rust/discussions/37)

<details>
  <summary>ServerStatus-theme 主题</summary>

作者 [@JingBh](https://github.com/JingBh)
👉 [主题地址](https://github.com/JingBh/ServerStatus-theme)
支持快速部署一键命令生成

| <img width="1269" alt="image" src="https://github.com/zdz/ServerStatus-Rust/assets/152173/33eb8685-b0ed-4548-92af-8cfdded7d011"> | <img width="596" alt="image" src="https://github.com/zdz/ServerStatus-Rust/assets/152173/15e9c405-6491-4f41-ad0e-68aae96d709c"> |
|-|-|

[演示：Demo](https://status.jingbh.cloud)

</details>

<details>
  <summary>ServerStatus-Theme-Light 主题</summary>

👉 [主题地址](https://github.com/orilights/ServerStatus-Theme-Light)
作者 [@orilights](https://github.com/orilights)

<img width="1836" alt="image" src="https://github.com/zdz/ServerStatus-Rust/assets/152173/35fdd138-31b8-46d0-8ea8-c2d4e7ef2b52">

[演示：Demo](https://sstl-demo.orilight.top)

</details>

<details>
  <summary>Hotaru 主题</summary>

Hotaru 主题由 [@HinataKato](https://github.com/HinataKato) 修改提供，[主题地址](https://github.com/HinataKato/hotaru_theme_for_RustVersion)

<img width="1202" alt="image" src="https://user-images.githubusercontent.com/152173/167900971-5ef0c23a-af43-4f52-aab5-d58e4a66c8ea.png">

</details>

<details>
  <summary>ServerStatus-web 主题</summary>

ServerStatus-web 主题由 [@mjjrock](https://github.com/mjjrock) 修改提供，[主题地址](https://github.com/mjjrock/ServerStatus-web)

<img width="1425" alt="image" src="https://user-images.githubusercontent.com/102237118/171837653-3a5b2cd6-bf02-4602-a132-2c80a6707f68.png">

</details>

<details>
  <summary>ServerStatus-nezha 主题</summary>

ServerStatus-nezha 主题由 [@snowie2000](https://github.com/snowie2000) 修改提供，类似于哪吒探针v1，[主题地址](https://github.com/snowie2000/serverstatus-nezha-theme)

<img width="1425" alt="image" src="https://github.com/user-attachments/assets/2f0a9ca1-0d7d-472c-bf0d-eada396f6219">

</details>


<details>
  <summary>v1.5.7 版本主题</summary>

[演示：Demo](https://tz-rust.vercel.app)

<img width="1215" alt="image" src="https://user-images.githubusercontent.com/152173/165957689-d35714a9-f7f8-49f7-9573-97d4cf3c2f79.png">
</details>

## 2. 安装部署

### 2.1 快速体验
```bash
# for CentOS/Debian/Ubuntu x86_64
mkdir -p /opt/ServerStatus && cd /opt/ServerStatus
# apt install -y unzip / yum install -y unzip
wget --no-check-certificate -qO one-touch.sh 'https://raw.githubusercontent.com/zdz/ServerStatus-Rust/master/scripts/one-touch.sh'
bash -ex one-touch.sh
# 部署完毕，打开 http://127.0.0.1:8080/ 或 http://<你的IP>:8080/
# 自定义部署可参照 scripts/one-touch.sh 脚本
```

### 2.2 快速部署

👉 [快速部署](https://doc.ssr.rs/rapid_deploy)

### 2.3 服务管理脚本

<details>
  <summary>服务管理脚本说明</summary>

由 [@Colsro](https://github.com/Colsro) &
[@Yooona-Lim](https://github.com/Yooona-Lim)  贡献

```bash
# 下载脚本
wget --no-check-certificate -qO status.sh 'https://raw.githubusercontent.com/zdz/ServerStatus-Rust/master/scripts/status.sh'

# 安装 服务端
bash status.sh -i -s

# 安装 客户端
bash status.sh -i -c
# or
bash status.sh -i -c protocol://username:password@master:port
# eg:
bash status.sh -i -c grpc://h1:p1@127.0.0.1:9394
bash status.sh -i -c http://h1:p1@127.0.0.1:8080

# 更多用法：
❯ bash status.sh

help:
    -i,--install    安装 Status
        -i -s           安装 Server
        -i -c           安装 Client
        -i -c conf      自动安装 Client
    -up,--upgrade   升级 Status
        -up -s          升级 Server
        -up -c          升级 Client
        -up -a          升级 Server和Client
    -un,--uninstall  卸载 Status
        -un -s           卸载 Server
        -un -c           卸载 Client
        -un -a           卸载 Server and Client
    -rc,--reconfig      更改 Status 配置
        -rc          更改 Client 配置
        -rc conf         自动更改 Client配置
    -s,--server     管理 Status 运行状态
        -s {status|start|stop|restart}
    -c,--client     管理 Client 运行状态
        -c {status|start|stop|restart}
    -b,--bakup      备份 Status
        -b -s          备份 Server
        -b -c          备份 Client
        -b -a          备份 Server and Client
    -rs,--restore    恢复 Status
        -rs -s          恢复 Server
        -rs -c          恢复 Client
        -rs -a          恢复 Server and Client
    -h,--help       查看帮助
若无法访问 Github:
    CN=true bash status.sh args
```

</details>

### 2.4 Railway 部署

懒得配置 `Nginx`，`SSL` 证书？试试
[在 Railway 部署 Server](https://github.com/zdz/ServerStatus-Rust/wiki/Railway)

[![Deploy on Railway](https://railway.app/button.svg)](https://railway.app/new/template/kzT46l?referralCode=pJYbdU)

### 2.5 Heroku 部署

[如何在 Heroku 上部署 Rust 版 ServerStatus 云探针](https://github.com/zdz/ServerStatus-Rust/blob/master/heroku/README.md)

## 3. 服务端说明

### 3.1 配置文件 `config.toml`
```toml
# 侦听地址, ipv6 使用 [::]:9394
grpc_addr = "0.0.0.0:9394"
http_addr = "0.0.0.0:8080"
# 默认30s无上报判定下线
offline_threshold = 30

# 管理员账号,不设置默认随机生成，用于查看 /detail, /map
admin_user = ""
admin_pass = ""

# hosts 跟 hosts_group 两种配置模式任挑一种配置即可
# name 主机唯一标识，不可重复，alias 为展示名
# notify = false 单独禁止单台机器的告警，一般针对网络差，频繁上下线
# monthstart = 1 没启用vnstat时，表示月流量从每月哪天开始统计
# disabled = true 单机禁用
# location 支持国旗 emoji https://emojixd.com/group/flags
# 或国家缩写，如 cn us 等等，所有国家见目录 web/static/flags
# 自定义标签 labels = "os=centos;ndd=2022/11/25;spec=2C/4G/60G;"
# os 标签可选，不填则使用上报数据，ndd(next due date) 下次续费时间, spec 为主机规格
# os 可用值 centos debian ubuntu alpine pi arch windows linux
hosts = [
  {name = "h1", password = "p1", alias = "n1", location = "🏠", type = "kvm", labels = "os=arch;ndd=2022/11/25;spec=2C/4G/60G;"},
  {name = "h2", password = "p2", alias = "n2", location = "🏢", type = "kvm", disabled = false},
  {name = "h3", password = "p3", alias = "n3", location = "🏡", type = "kvm", monthstart = 1},
  {name = "h4", password = "p4", alias = "n4", location = "cn", type = "kvm", notify = true, labels = "ndd=2022/11/25;spec=2C/4G/60G;"},
]

# 动态注册模式，不再需要针对每一个主机做单独配置
# gid 为模板组id, 动态注册唯一标识，不可重复
hosts_group = [
  # 可以按国家地区或用途来做分组
  {gid = "g1", password = "pp", location = "🏠", type = "kvm", notify = true},
  {gid = "g2", password = "pp", location = "🏢", type = "kvm", notify = true},
  # 例如不发送通知可以单独做一组
  {gid = "silent", password = "pp", location = "🏡", type = "kvm", notify = false},
]
# 动态注册模式下，无效数据清理间隔，默认 30s
group_gc = 30

# 不开启告警，可忽略后面配置，或者删除不需要的通知方式
# 告警间隔默认为30s
notify_interval = 30
# https://core.telegram.org/bots/api
# https://jinja.palletsprojects.com/en/3.0.x/templates/#if
[tgbot]
# 开关 true 打开
enabled = false
bot_token = "<tg bot token>"
chat_id = "<chat id>"
# host 可用字段见 payload.rs 文件 HostStat 结构, {{host.xxx}} 为占位变量
# 例如 host.name 可替换为 host.alias，大家根据自己的喜好来编写通知消息
# {{ip_info.query}} 主机 ip,  {{sys_info.host_name}} 主机 hostname
title = "❗<b>Server Status</b>"
online_tpl =  "{{config.title}} \n😆 {{host.location}} {{host.name}} 主机恢复上线啦"
offline_tpl = "{{config.title}} \n😱 {{host.location}} {{host.name}} 主机已经掉线啦"
# custom 模板置空则停用自定义告警，只保留上下线通知
custom_tpl = """
{% if host.memory_used / host.memory_total > 0.5  %}
<pre>😲 {{host.name}} 主机内存使用率超50%, 当前{{ (100 * host.memory_used / host.memory_total) | round }}%  </pre>
{% endif %}

{% if host.hdd_used / host.hdd_total  > 0.5  %}
<pre>😲 {{host.name}} 主机硬盘使用率超50%, 当前{{ (100 * host.hdd_used / host.hdd_total) | round }}% </pre>
{% endif %}
"""

# wechat, email, webhook 等其它通知方式 配置详细见 config.toml
```

### 3.2 服务端运行
```bash
# systemd 方式， 参照 scripts/one-touch.sh 脚本 (推荐)

# 💪 手动方式
# help
./stat_server -h
# 手动运行
./stat_server -c config.toml
# 或
RUST_BACKTRACE=1 RUST_LOG=trace ./stat_server -c config.toml

# 测试配置文件是否有效
./stat_server -c config.toml -t
# 根据配置发送测试消息，验证通知是否生效
./stat_server -c config.toml --notify-test

# 🐳 docker 方式
wget --no-check-certificate -qO docker-compose.yml 'https://raw.githubusercontent.com/zdz/ServerStatus-Rust/master/docker-compose.yml'
wget --no-check-certificate -qO config.toml 'https://raw.githubusercontent.com/zdz/ServerStatus-Rust/master/config.toml'
touch stats.json
docker-compose up -d
```

## 4. 客户端说明

<details>
  <summary>系统版本&架构</summary>

|  OS            | Release  |
|  ----          | ----     |
| Linux x86_64   | x86_64-unknown-linux-musl |
| Linux arm64    | aarch64-unknown-linux-musl |
| MacOS x86_64   | x86_64-apple-darwin |
| MacOS arm64    | aarch64-apple-darwin |
| Windows x86_64 | x86_64-pc-windows-msvc |
| Raspberry Pi   | armv7-unknown-linux-musleabihf |
| Android 64bit  | aarch64-linux-android |
| Android 32bit  | armv7-linux-androideabi |

</details>

### 4.1 Rust 版 Client
```bash
# 公网环境建议 headscale/nebula 组网或走 https, 使用 nginx 对 server 套 ssl 和自定义 location /report
# alpine linux 需要安装相关命令 apk add procps iproute2 coreutils
# 如果 Rust 版客户端在你的系统无法使用，请切换到下面 4.2 Python 跨平台版本

# systemd 方式， 参照 scripts/one-touch.sh 脚本 (推荐)

# 💪 手动方式
# Rust 版本 Client
./stat_client -h
./stat_client -a "http://127.0.0.1:8080/report" -u h1 -p p1
# 或
./stat_client -a "grpc://127.0.0.1:9394" -u h1 -p p1
# 不同的主机可以运行相同的命令注册到同一组
./stat_client -a "http://127.0.0.1:8080/report" -g g1 -p pp --alias "$(hostname)"

# rust client 可用参数
./stat_client -h
OPTIONS:
    -6, --ipv6                   ipv6 only, default:false
    -a, --addr <ADDR>            [default: http://127.0.0.1:8080/report]
        --alias <ALIAS>          alias for host [default: unknown]
        --cm <CM_ADDR>           China Mobile probe addr [default: cm.tz.cloudcpp.com:80]
        --ct <CT_ADDR>           China Telecom probe addr [default: ct.tz.cloudcpp.com:80]
        --cu <CU_ADDR>           China Unicom probe addr [default: cu.tz.cloudcpp.com:80]
        --disable-extra          disable extra info report, default:false
        --disable-notify         disable notify, default:false
        --disable-ping           disable ping, default:false
        --disable-tupd           disable t/u/p/d, default:false
    -g, --gid <GID>              group id [default: ]
    -h, --help                   Print help information
        --ip-info                show ip info, default:false
        --ip-source <IP_SOURCE>  ip info source [env: SSR_IP_SOURCE=] [default: ip-api.com]
        --sys-info               show sys info, default:false
        --json                   use json protocol, default:false
        --location <LOCATION>    location [default: ]
    -n, --vnstat                 enable vnstat, default:false
        --vnstat-mr <VNSTAT_MR>  vnstat month rotate 1-28 [default: 1]
    -p, --pass <PASS>            password [default: p1]
    -t, --type <HOST_TYPE>       host type [default: ]
    -u, --user <USER>            username [default: h1]
    -V, --version                Print version information
    -w, --weight <WEIGHT>        weight for rank [default: 0]

# 一些参数说明
--ip-info       # 显示本机ip信息后立即退出，目前使用 ip-api.com 数据
--ip-source     # 指定 ip 信息源，ip-api.com / ip.sb / ipapi.co / myip.la
--sys-info      # 显示本机系统信息后立即退出
--disable-extra # 不上报系统信息和IP信息
--disable-ping  # 停用三网延时和丢包率探测
--disable-tupd  # 不上报 tcp/udp/进程数/线程数，减少CPU占用
-w, --weight    # 排序加分，微调让主机靠前显示，无强迫症可忽略
-g, --gid       # 动态注册的组id
--alias         # 动态注册模式下，指定主机的展示名字
# 总流量，网卡流量/网速统计
-i, --iface         # 非空时，只统计指定网口
-e, --exclude-iface # 排除指定网口，默认排除 "lo,docker,vnet,veth,vmbr,kube,br-"
```

### 4.2 Python 版 Client

<details>
  <summary> Python 版 Client 说明</summary>

```bash
# Python 版本 Client 依赖安装
## Centos
yum -y install epel-release
yum -y install python3-pip gcc python3-devel
python3 -m pip install psutil requests py-cpuinfo

## Ubuntu/Debian
apt -y install python3-pip
python3 -m pip install psutil requests py-cpuinfo

## Alpine linux
apk add wget python3 py3-pip gcc python3-dev musl-dev linux-headers
apk add procps iproute2 coreutils
python3 -m pip install psutil requests py-cpuinfo

wget --no-check-certificate -qO stat_client.py 'https://raw.githubusercontent.com/zdz/ServerStatus-Rust/master/client/stat_client.py'

## Windows
# 安装 python 3.10 版本，并设置环境变量
# 命令行执行 pip install psutil requests
# 下载 https://raw.githubusercontent.com/zdz/ServerStatus-Rust/master/client/stat_client.py
pip install psutil requests py-cpuinfo

python3 stat_client.py -h
python3 stat_client.py -a "http://127.0.0.1:8080/report" -u h1 -p p1
```
</details>

## 5. 开启 `vnstat` 支持
[vnstat](https://zh.wikipedia.org/wiki/VnStat) 是Linux下一个流量统计工具，开启 `vnstat` 后，`server` 完全依赖客户机的 `vnstat` 数据来显示月流量和总流量，优点是重启不丢流量数据。

<details>
  <summary>开启 vnstat 设置</summary>

```bash
# 在client端安装 vnstat
## Centos
sudo yum install epel-release -y
sudo yum install -y vnstat
## Ubuntu/Debian
sudo apt install -y vnstat

# 修改 /etc/vnstat.conf
# BandwidthDetection 0
# MaxBandwidth 0
# 默认不是 eth0 网口的需要置空 Interface 来自动选择网口
# 没报错一般不需要改
# Interface ""
systemctl restart vnstat

# 确保 version >= 2.6
vnstat --version
# 测试查看月流量 (刚安装可能需等一小段时间来采集数据)
vnstat -m
vnstat --json m

# client 使用 -n 参数开启 vnstat 统计
./stat_client -a "grpc://127.0.0.1:9394" -u h1 -p p1 -n
# 或
python3 stat_client.py -a "http://127.0.0.1:8080/report" -u h1 -p p1 -n
```
</details>

## 6. FAQ

<details>
  <summary>如何使用自定义主题</summary>

更简单的方式 👉 [#37](https://github.com/zdz/ServerStatus-Rust/discussions/37)

```nginx
server {
  # ssl, domain 等其它 nginx 配置

  # 自动反代所有请求
  location @proxy {
    proxy_set_header Host              $host;
    proxy_set_header X-Real-IP         $remote_addr;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Host  $host;
    proxy_set_header X-Forwarded-Port  $server_port;
		proxy_set_header Upgrade $http_upgrade;
		proxy_set_header Connection $http_connection;

    proxy_pass http://127.0.0.1:8080;
  }

  # 如果主题存在相关文件则使用，否则回退到上游
  location / {
    root   /opt/ServerStatus/web; # 你自己修改的主题目录
    index  index.html index.htm;
    try_files $uri $uri/ @proxy;
  }
}
```
</details>

<details>
  <summary>如何源码编译</summary>

```bash
#
cargo install stat_server
cargo install stat_client
# or
# 按提示安装 rust 编译器
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
yum install -y openssl-devel
git clone https://github.com/zdz/ServerStatus-Rust.git
cd ServerStatus-Rust
cargo build --release
# 编译好的文件目录 target/release
```
</details>

<details>
  <summary>如何自定义 ping 地址</summary>

```bash
# 例如自定义移动探测地址，用 --cm 指定地址
./stat_client -a "grpc://127.0.0.1:9394" -u h1 -p p1 --cm=cm.tz.cloudcpp.com:80

# 电信联通参数可以使用 -h 命令查看
./stat_client -h
OPTIONS:
    --cm <CM_ADDR>    China Mobile probe addr [default: cm.tz.cloudcpp.com:80]
    --ct <CT_ADDR>    China Telecom probe addr [default: ct.tz.cloudcpp.com:80]
    --cu <CU_ADDR>    China Unicom probe addr [default: cu.tz.cloudcpp.com:80]
```
</details>

<details>
  <summary>关于这个轮子</summary>

  之前一直在使用 `Prometheus` + `Grafana` + `Alertmanager` + `node_exporter` 做VPS监控，这也是业界比较成熟的监控方案，用过一段时间后，发现非生产环境，很多监控指标都用不上，运维成本有点大。
  而 `ServerStatus` 很好，足够简单和轻量，一眼可以看尽所有小机机，只是 `c++` 版本很久没迭代过，自己的一些需求在原版上不是很好修改，如自带 `tcp` 上报对跨区机器不是很友好，也不方便对上报的链路做优化 等等。这是学习 `Rust` 练手的小项目，所以不会增加复杂功能，保持小而美，简单部署，配合 [Uptime Kuma](https://github.com/louislam/uptime-kuma) 基本上可以满足个人大部分监控需求。

</details>

## 7. 相关项目
- https://github.com/BotoX/ServerStatus
- https://github.com/cppla/ServerStatus
- https://github.com/mojeda/ServerStatus
- https://github.com/cokemine/ServerStatus-Hotaru
- https://github.com/ToyoDAdoubiBackup/ServerStatus-Toyo

## 8. 最后

    很高兴我的代码能跑在你的服务器上，如果对你有帮助的话，欢迎留下你的 star ⭐ 支持一下

## Fork 新增功能（本分支 `port/traffic-admin-ping`）

上游功能全部保留；以下为本分支在上游基础上的改动。部署时服务端与客户端均从本分支源码构建，**不要混用上游 Release 二进制**。

### 后台管理 `/admin`

用 `config.toml` 的 admin 账号登录，改动即时生效、无需重启，持久化在服务端 `admin.json`（0600），不改写 `config.toml`：

- **节点表**：增删改节点、拖动排序（拖完点保存）、国旗图片、IP 两行显示 + 复制按钮、年费货币（¥ / $ / £ / € / 自定义）、带宽单位（Mbps / Gbps / Tbps，存 Mbps、自适应显示）、开始 / 结束日期、公开备注行内改、一键命令重新获取。
- **分组**：后台可建分组模板（地区、类型、流量限额、套餐默认值）；客户端用 `-g 分组名 -p 分组密码` 接入即自动建节点；删除分组不踢出已注册成员。
- **套餐**：节点与分组均可设开始日期、年费、带宽、流量限额；分组模板的套餐由新成员继承。
- **流量修正**：表单填的是「已用」（按计费模式折算的量，不是入 + 出之和）；修正后已用精确等于填的值，之后采集往上加，总量只升不降且恒 ≥ 本月。
- **三网 ping 设置**：后台改联通 / 电信 / 移动探针地址与开关，客户端下次上报自动生效，不用逐台改命令行。
- **掉线推送 PushPlus**：后台填 token 即可启用（token 只存服务端、接口不回显），掉线 / 恢复有默认文案，模板为空时自定义 / 流量告警不发。

### 流量统计与口径

- 服务端累计日 / 月 / 总量，计费模式 `sum | up | down | max`（up 只计出方向、down 只计入方向、max 取大、空或未知归 sum），重置日 1–31 号（留空为自然月），重启重对齐与补记，额度告警（默认 80% 与 100% 两档），状态存 `traffic.json`。vnstat 节点保持原路，老客户端走旧基线。
- **口径**：主面板「总流量」为网卡开机累计（`network_in / network_out`）；「月流量」为本月累计（`month_rx / month_tx`）；详情行只显示本月三项：已用 · 剩余 · 每月限制。
- **不限量标记：`0` 表示不限量**（旧 `9999` 约定已退役，填 9999 即字面 9999 GiB）。
- **结束日期绝不推导**：只显示后台填过的值，空永远是空；外部改动 `admin.json` 必须先停服务、改完再启动，否则下次后台保存会把内存旧值整文件写回。

### 三网探针：ICMP

探针为 ICMP echo，只测到主机的往返，不再依赖目标端口；配置里的端口自动剥离，旧地址无需重填。客户端 DGRAM 优先、RAW 兜底，打不开 socket 记丢包，**绝不回落 TCP**。探针历史由服务端持久化（六路分钟桶 25 小时 + 小时桶 8 天，桶内均值），公开接口 `GET /api/ping-history?host=&range=1h|6h|24h|7d`；历史只能从部署起攒，不编造。

### 展开详情（主面板）

点行展开后只显示摘要行 + 「网络延迟 / 系统概览」两个标签页（原生展开块已移除）：

- 延迟图档位 1h / 6h / 24h / 7d，纵轴取整、横轴整点锚定加密；悬停竖排提示，竖虚线与提示列严格对齐、鼠标与触屏均可拖动 scrub，松手后停留到下一次悬停移动；断档列自动吸附到最近有值列。
- 一次只能展开一个节点（手风琴）；暗模式下图表卡为深底浅字；行点击展开 / 收起由 React 自身 toggle，整行任意格子点击均可。

### 前端定制方式

仓库里**没有 React 源码**（只有编译产物），表格类定制走 overlay 注入：`web/` 下的 overlay 脚本经 RustEmbed 编进服务端二进制，改前端必须重构建服务端 + 重部署。版本化规则：overlay 用路径版本号（`plan-overlay-vNN.js`，新文件、旧文件保留防旧缓存 404）；bundle 用内容 md5 前 8 位新文件名。部署后必须公网按字节比对带版本号的文件，容器本机正确不等于浏览器拿到正确版。

### 构建说明

- 服务端为 glibc 动态版，Docker 底包须用 `ubuntu:24.04`；构建前先 `touch server/build.rs`，否则 banner 里的提交号可能缓存成旧值。
- 客户端分发**只用 musl 静态版**（`x86_64-unknown-linux-musl`），动态版在 glibc 较低的机器上起不来。
- 启动日志不打印后台账号与密码；鉴权响应字段名为 `access_token`。


## 从零部署（全新部署）

本节是从一台空机器开始、把本分支完整跑起来的步骤，分服务端、反代、后台初始化、客户端四步。服务端与客户端均从本分支源码构建，**不要混用上游 Release 二进制**：上游客户端没有本分支的 ICMP 探针与服务端下发探针，上游服务端没有后台、流量累计与探针历史。前端定制随服务端二进制一起发布（overlay 经 RustEmbed 编进二进制），无需单独部署前端。

本分支暂不提供可用的预编译镜像，一律本地构建。完整 release 构建建议 2GB 以上内存；小内存机器先加 swap，或在别的机器上编好再把二进制拷过来。

### 1. 服务端：Docker 方式（推荐）

```bash
git clone <本仓库地址>
cd ServerStatus-Rust
mkdir -p data
```

按第 3 节写好 `data/config.toml`（`data` 里只手写这一个文件；`stats.json`、`traffic.json`、`admin.json`、`ping_history.json` 由服务端自动生成，缺失按空加载，**不要手建空 JSON 文件**，空文件不是合法 JSON）。

在仓库根目录新建 `docker-compose.yml`：

```yaml
services:
  ssr-server:
    build:
      context: .
      dockerfile: Dockerfile.ghcr
    image: ssr-server:latest
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
docker compose up -d --build
```

首次构建约 10–20 分钟（在镜像里编整个 workspace）。启动后打开 `http://<服务端IP>:8080`，能看到面板即服务端已起。

### 2. 服务端：裸二进制方式（备选）

```bash
git clone <本仓库地址>
cd ServerStatus-Rust
touch server/build.rs   # 否则 banner 里的提交号可能缓存成旧值
cargo build --release -p stat_server
```

产物在 `target/release/stat_server`。把它和仓库里的 `systemd/stat_server.service` 配合使用（只改下面两行）：

```ini
[Service]
WorkingDirectory=/opt/ServerStatus
ExecStart=/opt/ServerStatus/stat_server -c /opt/ServerStatus/config.toml
Restart=on-failure
```

即：二进制放 `/opt/ServerStatus/stat_server`，`config.toml` 与数据文件放 `/opt/ServerStatus/`（WorkingDirectory 指到哪里，数据文件就写到哪里），然后：

```bash
cp target/release/stat_server /opt/ServerStatus/
cp systemd/stat_server.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now stat_server
```

### 3. 最小 config.toml

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

# 或者分组模式（二选一）：客户端用 -g <gid> -p <分组密码> 接入即自动建节点
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

另注意两点：`server_url` 不用填，服务端按请求 Host 自动生成——所以反代必须透传 Host，否则面板的 `/i` 一键命令会生成内网地址；`admin.json` 是后台的持久化文件，**外部直接改它必须先停服务**，改完再启动，否则下次后台保存会把内存旧值整文件写回。

### 4. 反代 + TLS（nginx 示例）

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

证书用 certbot 申请（HTTP-01 需 80 端口可达，DNS-01 适合已有域名解析），路径按自己环境改。公网只需暴露 80/443；8080/9394 保持本机监听即可。

### 5. 后台初始化（/admin）

打开 `https://<你的域名>/admin`，用 `config.toml` 的 admin 账号登录：

1. 建分组 / 节点（或等客户端自动注册）。
2. 填三网探针地址（联通 / 电信 / 移动 × v4 / v6，见第 3 节格式）。
3. PushPlus：填**自己的** token，开关与模板按需改（token 只存服务端，接口不回显）。
4. 套餐：限额、模式、重置日、开始日期、年费、带宽按需填；**结束日期只填已知值，绝不推导，空永远是空**。
5. 后台改动即时生效，不用重启服务端。

### 6. 客户端（musl 手动安装；不要用面板的 /i 一键脚本）

`/i` 下发的一键脚本从上游 Release 下载客户端（仅 systemd 服务），没有本分支的 ICMP 探针与服务端下发探针；要完整功能必须手动装 musl 版。

构建（构建机上做，不必是节点本身）：

```bash
rustup target add x86_64-unknown-linux-musl
# Debian/Ubuntu：apt install -y musl-tools   # 提供 musl-gcc
cargo build --release --target x86_64-unknown-linux-musl -p stat_client
```

产物在 `target/x86_64-unknown-linux-musl/release/stat_client`。拷到节点：

```bash
mkdir -p /opt/ServerStatus
cp target/x86_64-unknown-linux-musl/release/stat_client /opt/ServerStatus/stat_client
```

systemd（参照仓库 `systemd/stat_client.service`，只改 ExecStart 一行）：

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

Alpine/OpenRC 机器：把上面同一行 ExecStart 写进 `/etc/init.d/stat_client`（openrc-run 格式，`command=/opt/ServerStatus/stat_client`，`command_args` 照抄引号里的参数），`rc-update add stat_client default && rc-service stat_client start`。

客户端须以 root 运行（ICMP 需 DGRAM/RAW 权限；非 root 需配 `ping_group_range` 或 `CAP_NET_RAW`）。节点名 / 分组名与密码须与 `config.toml` 或后台一致，否则注册不上。

### 7. 验收清单

- 面板 `https://<你的域名>` 能打开，节点在线（30 秒无上报判下线）。
- `/admin` 能登录，后台改动即时生效。
- 探针格有数值；空配置的列显示 — 属正常未配置，不是故障。
- 客户端 `systemctl status stat_client` 无持续报错，月流量随上报增长。
- 浏览器看到旧版先硬刷新：overlay 每页只加载一次，旧标签页要重载才拿新版。

### 8. 常见坑

- 限额填 `0` = 不限量，不要填 `9999`。
- 结束日期不推导；`admin.json` 外改先停服务。
- 数据目录不要手建空 JSON 文件。
- 小内存机器构建 OOM：加 swap 或异地构建再拷二进制。
- 旧 overlay / bundle 文件不要删：旧缓存的 HTML 还引用它们，删了会 404。
- 探针历史只能从部署起攒，过去数据不编造；延迟图档位 1h / 6h / 24h / 7d。
