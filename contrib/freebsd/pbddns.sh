#!/bin/sh

# PROVIDE: pbddns
# REQUIRE: NETWORKING
# BEFORE: LOGIN
# KEYWORD: nojail

. /etc/rc.subr

name="pbddns"
rcvar=pbddns_enable

load_rc_config $name

pidfile="/var/run/${name}.pid"

: ${pbddns_enable:=NO}
: ${pbddns_flags:=""}

pbddns_env_file=/usr/local/etc/pbddns.conf

start_cmd="${name}_start"
status_cmd="${name}_status"
stop_cmd="${name}_stop"

extra_commands="status"

pbddns_start()
{
    info "Starting ${name}."
    /usr/local/bin/pbddns --write-pid=${pidfile} ${pbddns_flags} | logger -t pbddns &
}


pbddns_status()
{
    if [ -f ${pidfile} ]; then
        pid=`cat ${pidfile}`
        if ps -p ${pid} | grep -q ${pid}; then
            echo "${name} is running as pid ${pid}."
        else
            echo "${name} is not running (pidfile exists)."
        fi
    else
        echo "${name} is not running."
    fi
}

pbddns_stop()
{
    info "Stopping ${name}."
    if [ -f ${pidfile} ]; then
        pid=`cat ${pidfile}`
        if ps -p ${pid} | grep -q ${pid}; then
            kill ${pid}
        else
            echo "${name} is not running (pidfile exists)."
        fi
    else
        echo "${name} is not running."
    fi
}

run_rc_command "$1"