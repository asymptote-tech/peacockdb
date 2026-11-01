# shellcheck shell=bash
#
# The S3 storage every bucket script passes to the aws CLI: the endpoint and region come from
# AWS_ENDPOINT_URL and AWS_REGION, the names the CLI itself reads, so the storage moves with the
# keys file and no script names a provider's endpoint. Access is the S3 API with static keys.

# s3_args <array> — sets <array> to `--endpoint-url <url> --region <region>`. With either variable
# unset or empty, names it and returns 2: without an endpoint every call would go to AWS.
s3_args() {
  local -n s3_out=$1
  local name
  for name in AWS_ENDPOINT_URL AWS_REGION; do
    if [ -z "${!name:-}" ]; then
      echo "error: $name is not set; the S3 storage is AWS_ENDPOINT_URL and AWS_REGION" >&2
      return 2
    fi
  done
  s3_out=(--endpoint-url "$AWS_ENDPOINT_URL" --region "$AWS_REGION")
}
