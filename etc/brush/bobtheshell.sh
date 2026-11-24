# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# bobthefish-style prompt for brush. The segment line carries the exit status, the folded
# path and the git branch; the last line stays the stable `raven# ` input marker so serial
# transcripts and scripted gates keep working.
#
# Stable modes:
#   RAVEN_PROMPT=plain or RAVEN_PLAIN_PROMPT=1 -> `raven# ` (no ANSI, transcript-safe)
#   NO_COLOR or TERM=dumb                      -> `user@host:folded-path# `, no ANSI
#   RAVEN_PROMPT_POWERLINE=1                   -> U+E0B0 separator and branch glyph

[[ -n ${RAVEN_BOBTHESHELL_LOADED:-} ]] && return
RAVEN_BOBTHESHELL_LOADED=1

raven_prompt_path() {
    local path=${PWD:-}
    local prefix=''
    local segment
    local out=''

    if [[ $path == "$HOME" ]]; then
        printf '~'
        return
    fi
    if [[ $path == "$HOME"/* ]]; then
        prefix='~/'
        path=${path#"$HOME"/}
    elif [[ $path == /* ]]; then
        prefix='/'
        path=${path#/}
    fi
    while [[ $path == */* ]]; do
        segment=${path%%/*}
        path=${path#*/}
        if [[ -n $segment ]]; then
            # Fold every intermediate component to its first character.
            out+="${segment%"${segment#?}"}/"
        fi
    done
    printf '%s%s%s' "$prefix" "$out" "$path"
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
    printf '%s%s' "$branch" "$dirty"
}

raven_prompt_jobs() {
    local count=0
    local job
    for job in $(jobs -p); do
        count=$((count + 1))
    done
    [[ $count -gt 0 ]] && printf '%s' "$count"
}

raven_prompt() {
    local status=$?
    local mode=${RAVEN_PROMPT:-}

    if [[ $mode == plain || ${RAVEN_PLAIN_PROMPT:-0} == 1 ]]; then
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

    local powerline=${RAVEN_PROMPT_POWERLINE:-0}
    local text=''
    local segments=''

    if [[ $status -ne 0 ]]; then
        text+="${status} "
        segments+="\[\e[1;31m\]${status}\[\e[0m\] "
    fi

    text+="${path}"
    segments+="\[\e[38;5;183m\]${path}\[\e[0m\]"

    local git
    git=$(raven_prompt_git)
    if [[ -n $git ]]; then
        if [[ $powerline == 1 ]]; then
            text+=" ${git}"
            segments+=" \[\e[1;32m\]\ue0a0 ${git}\[\e[0m\]"
        else
            text+=" git:${git}"
            segments+=" \[\e[1;32m\]git:${git}\[\e[0m\]"
        fi
    fi

    local venv
    if [[ -n ${VIRTUAL_ENV:-} ]]; then
        venv=${VIRTUAL_ENV##*/}
        text+=" ${venv}"
        segments+=" \[\e[38;5;45m\]${venv}\[\e[0m\]"
    fi

    local jobs
    jobs=$(raven_prompt_jobs)
    if [[ -n $jobs ]]; then
        text+=" jobs:${jobs}"
        segments+=" \[\e[38;5;220m\]jobs:${jobs}\[\e[0m\]"
    fi

    RAVEN_PROMPT_TEXT="${text}
raven${root} "
    PS1="${segments}
\[\e[38;5;183m\]raven${root} \[\e[0m\]"
}

PROMPT_COMMAND=raven_prompt
raven_prompt
