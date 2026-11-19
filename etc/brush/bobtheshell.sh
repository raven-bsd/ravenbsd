# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# bobtheshell prompt for brush, ported from the owner's zsh theme without zsh escapes,
# vcs_info or an oh-my-zsh/oh-my-bash framework. Source this file in an interactive brush:
#
#   . /etc/brush/bobtheshell.sh
#
# Stable modes:
#   RAVEN_PROMPT=plain       -> `raven# ` (no ANSI, transcript-safe)
#   NO_COLOR or TERM=dumb    -> `user@host:path# ` with no ANSI
#   RAVEN_PROMPT_POWERLINE=1 -> U+E0B0 separators; the flat `|` fallback otherwise
#
# RAVEN_PROMPT_TEXT always holds the visible (colour-free) prompt text so tests and
# terminal-title helpers do not have to parse the PS1 escape syntax.

[[ -n ${RAVEN_BOBTHESHELL_LOADED:-} ]] && return
RAVEN_BOBTHESHELL_LOADED=1

raven_prompt_path() {
    case "$PWD" in
        "$HOME") printf '~' ;;
        "$HOME"/*) printf '~/%s' "${PWD##*/}" ;;
        /) printf '/' ;;
        *) printf '%s' "${PWD##*/}" ;;
    esac
}

raven_prompt_git() {
    local git_bin
    git_bin=$(command -v git) || return
    [[ -n $git_bin ]] || return

    local inside
    inside=$(git rev-parse --is-inside-work-tree 2>&1) || return
    [[ $inside == true ]] || return

    local branch
    branch=$(git symbolic-ref --short HEAD 2>&1) ||
        branch=$(git rev-parse --short HEAD 2>&1)
    [[ -n $branch ]] || return

    local dirty=
    local porcelain
    porcelain=$(git status --porcelain 2>&1)
    [[ -n $porcelain ]] && dirty='*'
    printf ' %s%s' "$branch" "$dirty"
}

raven_prompt_jobs() {
    local count=0
    local job
    for job in $(jobs -p); do
        count=$((count + 1))
    done
    [[ $count -gt 0 ]] && printf ' jobs:%s' "$count"
}

raven_prompt() {
    local status=$?
    local mode=${RAVEN_PROMPT:-}

    if [[ $mode == plain ]]; then
        PS1='raven# '
        RAVEN_PROMPT_TEXT=$PS1
        return
    fi

    local user=${USER:-root}
    local host=${HOSTNAME:-$(hostname 2>&1)}
    local path
    path=$(raven_prompt_path)
    local root='$'
    [[ ${EUID:-$(id -u 2>&1)} == 0 ]] && root='#'

    if [[ -n ${NO_COLOR:-} || ${TERM:-} == dumb ]]; then
        PS1="${user}@${host}:${path}${root} "
        RAVEN_PROMPT_TEXT=$PS1
        return
    fi

    local git
    git=$(raven_prompt_git)
    local venv=
    [[ -n ${VIRTUAL_ENV:-} ]] && venv=" ${VIRTUAL_ENV##*/}"
    local fail=
    [[ $status -ne 0 ]] && fail=" !$status"
    local sep='|'
    if [[ ${RAVEN_PROMPT_POWERLINE:-0} == 1 ]]; then
        sep=$'\ue0b0'
    fi
    local jobs
    jobs=$(raven_prompt_jobs)

    RAVEN_PROMPT_TEXT="${user}@${host} ${sep} ${path}${git}${venv}${fail}${jobs} ${root} "
    PS1="\[\e[38;5;183m\]${user}@${host}\[\e[0m\] ${sep} \[\e[38;5;231m\]${path}\[\e[0m\]${git}${venv}${fail}${jobs} \[\e[38;5;183m\]${root}\[\e[0m\] "
}

PROMPT_COMMAND=raven_prompt
raven_prompt
