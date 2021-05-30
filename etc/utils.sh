function print_error() {
  tput setaf 1 && echo "Error: ${1}"
}

function print_success() {
  tput setaf 2 && echo "Success: ${1}"
}

function print_info() {
  tput setaf 4 && echo "Info: ${1}"
}

function command_exists() {
  command=${1}

  if type ${command}; then
    :
  else
    exit 1
  fi
}
