function export_vars_for_environment {
  local environment=$1
  local env_file="crates/collab/k8s/environments/${environment}.sh"
  if [[ ! -f $env_file ]]; then
    echo "Invalid environment name '${environment}'" >&2
    exit 1
  fi
  local line
  while IFS= read -r line || [[ -n "$line" ]]; do
    case "$line" in
      ''|'#'*) continue ;;
    esac
    if [[ ! "$line" =~ ^[A-Z][A-Z0-9_]*=[A-Za-z0-9._:/-]+$ ]]; then
      echo "Invalid environment assignment in '$env_file'" >&2
      exit 1
    fi
    export "$line"
  done < "$env_file"
}

function target_orion_kube_cluster {
  if [[ $(kubectl config current-context 2> /dev/null) != do-nyc1-orion-1 ]]; then
    doctl kubernetes cluster kubeconfig save orion-1
  fi
}

function tag_for_environment {
  local environment=$1
  if [[ "$environment" == "production" ]]; then
    echo "collab-production"
  elif [[ "$environment" == "staging" ]]; then
    echo "collab-staging"
  else
    echo "Invalid environment name '${environment}'" >&2
    exit 1
  fi
}

function url_for_environment {
  local environment=$1
  if [[ "$environment" == "production" ]]; then
    echo "https://collab.orion.dev"
  elif [[ "$environment" == "staging" ]]; then
    echo "https://collab-staging.orion.dev"
  else
    echo "Invalid environment name '${environment}'" >&2
    exit 1
  fi
}
