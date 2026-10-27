#!/bin/bash
# ServerStatus-Rust fork client init script
# Downloads the fork's musl stat_client from GitHub Releases and installs
# it as a systemd service. Probes are distributed by the server, so the
# install command needs no cm/ct/cu options.
# Served by the panel's /i endpoint; run it on the node as root.

export SSR_WORKSPACE={{workspace}}

Info="\033[32m[info]\033[0m"
Error="\033[31m[err]\033[0m"

mkdir -p ${SSR_WORKSPACE}
cd ${SSR_WORKSPACE}

if [ "${DBG}" = "1" ]; then
    set -x
fi

function say() {
    printf "${Info} ssr-client-init: %s\n" "$1"
}

function err() {
    printf "${Error} ssr-client-init: %s\n" "$1" >&2
    exit 1
}

function check_cmd() {
    command -v "$1" > /dev/null 2>&1
}

function need_cmd() {
    if ! check_cmd "$1"; then
        err "need '$1' (command not found)"
    fi
}

# fork Release 目前只有 x86_64 构建，ARM 请按指南手动安装
function check_arch() {
    need_cmd uname

    case $(uname -m) in
        x86_64)
            say "os arch: x86_64"
        ;;
        *)
            err "fork 一键脚本目前只支持 x86_64（Release 暂无 ARM 构建），ARM 机器请按指南手动安装"
            exit 1
        ;;
    esac
}

function download_client() {
    cd ${SSR_WORKSPACE}

    say "start download the stat_client"
    if check_cmd curl; then
        curl -L -o stat_client "https://github.com/ypkin/ServerStatus-Rust/releases/latest/download/stat_client" || err "下载失败，请检查网络（GitHub 是否可达）"
    elif check_cmd wget; then
        wget -O stat_client "https://github.com/ypkin/ServerStatus-Rust/releases/latest/download/stat_client" || err "下载失败，请检查网络（GitHub 是否可达）"
    else
        err "need 'curl' or 'wget' (command not found)"
    fi
    if [ ! -s stat_client ]; then
        err "下载到的文件为空，请检查网络后重试"
    fi

    say "download stat_client succ"

    say "try stop stat_client.service"
    systemctl stop stat_client > /dev/null 2>&1 || true

    chmod +x ${SSR_WORKSPACE}/stat_client
}

function install_client_service() {
    need_cmd cat
    need_cmd systemctl
    need_cmd sleep

    say "start install stat_client.service"

    cat > /etc/systemd/system/stat_client.service <<-EOF
[Unit]
Description=ServerStatus-Rust Client
After=network.target

[Service]
User=root
Group=root
Environment="RUST_BACKTRACE=1"
WorkingDirectory={{workspace}}
ExecStart={{workspace}}/stat_client {{client_opts}}
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure

[Install]
WantedBy=multi-user.target

EOF

    say "systemctl daemon-reload"
    systemctl daemon-reload
    say "start stat_client.service"
    systemctl start stat_client
    say "enable stat_client.service"
    systemctl enable stat_client

    sleep 2
    say "status stat_client.service"
    systemctl status stat_client
}

check_arch
need_cmd chmod
need_cmd systemctl
download_client
install_client_service
