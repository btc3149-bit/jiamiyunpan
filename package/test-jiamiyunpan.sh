#!/usr/bin/env bash

set -eo pipefail
set -x

case $1 in
  post-install)
    echo -e "\nJIAM VERSION:"
    jiamiyunpan --version
    echo -e "\nJIAM CONF:"
    cat /etc/jiamiyunpan.conf
    echo -e "\nJIAM SERVICE STATUS BEFORE ENABLE:"
    systemctl status jiamiyunpan || true
    echo -e "\nENABLE JIAM SERVICE:"
    systemctl enable jiamiyunpan
    echo -e "\nJIAM SERVICE STATUS AFTER ENABLE:"
    systemctl status jiamiyunpan || true
    echo -e "\nSTART JIAM SERVICE:"
    systemctl start jiamiyunpan
    
    echo -e "\nJIAM SERVICE STATUS AFTER START:"
    sleep 1s
    systemctl status jiamiyunpan
    ;;

  post-upgrade)
    echo -e "\nJIAM VERSION:"
    jiamiyunpan --version

    echo -e "\nJIAM CONF:"
    cat /etc/jiamiyunpan.conf

    echo -e "\nJIAM SERVICE STATUS:"
    systemctl status jiamiyunpan || true
    ;;
esac