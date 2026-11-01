# shellcheck shell=bash
#
# A chain as the board (llm-wiki/tasks/tasks.md) states it: `## Chain <c> (base: <b>)`, then one
# `### <n>. [...](<task>.md) — state: <s>` heading a task (llm-wiki/prompts.md, "Board protocol").
# The board names no branch: up.sh is told the branch of a chain that ran before, and names a new
# chain's first branch itself. Each function reads a board on stdin; one that refuses prints a line
# on stderr and returns 2.

# board_chain_section <chain> <where> [<hint>] — sets BOARD_BASE and BOARD_TASK, the chain's base
# and its lowest-numbered task, either empty where the section names none. Refuses a chain the
# board at <where> lacks, adding <hint>, or names twice.
board_chain_section() {
  local chain=$1 where=$2 hint=${3:+: $3} sections fields
  fields=$(awk -v c="$chain" '
    /^## / {
      inside = $0 ~ ("^## Chain " c "([ (]|$)")
      if (inside) {
        sections++
        base = match($0, /\(base: [^)]+\)/) ? substr($0, RSTART + 7, RLENGTH - 8) : ""
      }
      next
    }
    inside && /^### [0-9]+\. \[/ && match($0, /\]\([^)]+\.md\)/) {
      n = $2 + 0
      if (task == "" || n < low) { low = n; task = substr($0, RSTART + 2, RLENGTH - 6) }
    }
    END { print sections + 0 "|" base "|" task }')
  IFS='|' read -r sections BOARD_BASE BOARD_TASK <<< "$fields"
  case "$sections" in
    0) echo "${0##*/}: $where has no chain $chain$hint" >&2; return 2 ;;
    1) ;;
    *) echo "${0##*/}: $where has $sections sections for chain $chain" >&2; return 2 ;;
  esac
}

# board_chain <chain> <ref> — board_chain_section of the board at <ref>, a new chain's, refusing too
# a section with no task or no base.
board_chain() {
  local where="$2's board (llm-wiki/tasks/tasks.md)"
  board_chain_section "$1" "$where" "define the chain on $2 first"
  if [ -z "$BOARD_TASK" ]; then
    echo "${0##*/}: chain $1 on $where has no task" >&2
    return 2
  fi
  if [[ ! "$BOARD_TASK" =~ ^[A-Za-z0-9._-]+$ ]] || [[ ! "$BOARD_BASE" =~ ^[A-Za-z0-9._/-]+$ ]]; then
    echo "${0##*/}: chain $1 on $where names no (base: <branch>), or a first task that is no branch name" >&2
    return 2
  fi
}

# board_ran <chain> <task> — status 0 when the chain's section holds the heading of <task>.md at a
# state past `approved to build`. blocked(…) and rebase needed(…) count as the state they carry;
# words after a state, `approved to build (resumed …)` say, do not change it.
board_ran() {
  awk -v c="$1" -v t="$2" '
    /^## / { inside = $0 ~ ("^## Chain " c "([ (]|$)"); next }
    inside && /^### [0-9]+\. \[/ && index($0, "](" t ".md)") {
      s = match($0, /state: [^—]+/) ? substr($0, RSTART + 7, RLENGTH - 7) : ""
      if (s ~ /^(blocked|rebase needed)\(/) s = substr(s, index(s, "(") + 1)
      if (s ~ /^(building|reviewing|completing|completeness approved|done)([^a-z]|$)/) found = 1
    }
    END { exit !found }'
}
