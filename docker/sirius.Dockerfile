# The Sirius run image (llm-wiki/tasks/sirius-bench.md): Sirius `stable`'s prebuilt extension, the
# Python DuckDB it was built for, its config and the runner, docker/sirius-bench.py. No libcudf: the
# extension links it statically and needs only the driver's libcuda and libnvidia-ml, mounted by
# `docker run --gpus all`, and the C and C++ runtime. Built by scripts/env/image.sh, which passes
# every docker/pins.env line as a build arg and the storage keys as BuildKit secrets. The keys are
# mounted for the one RUN that fetches, in a stage the image takes one file from: no layer, label
# or input holds them.

# Python and the venv, at the paths the image runs them from.
FROM ubuntu:24.04 AS python
ARG PYTHON_VERSION
ARG PYTHON_STANDALONE_TAG
ARG PYTHON_SHA256
ARG SIRIUS_DUCKDB
ARG PYARROW_VERSION
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
COPY docker/sirius-venv.lock /tmp/venv.lock
RUN curl -fsSL -o /tmp/python.tar.gz \
      "https://github.com/astral-sh/python-build-standalone/releases/download/${PYTHON_STANDALONE_TAG}/cpython-${PYTHON_VERSION}%2B${PYTHON_STANDALONE_TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz" \
 && echo "${PYTHON_SHA256}  /tmp/python.tar.gz" | sha256sum -c - \
 && mkdir /opt/python && tar -xzf /tmp/python.tar.gz -C /opt/python --strip-components=1 && rm /tmp/python.tar.gz \
 && /opt/python/bin/python3 -m venv /opt/venv \
 && /opt/venv/bin/pip install --no-cache-dir -q --require-hashes --no-deps -r /tmp/venv.lock \
 && diff <(/opt/venv/bin/pip freeze) <(sed -n 's/ --hash=.*//p' /tmp/venv.lock) \
 && [ "$(/opt/venv/bin/python -c 'import duckdb, pyarrow; print(duckdb.__version__, pyarrow.__version__)')" \
      = "${SIRIUS_DUCKDB} ${PYARROW_VERSION}" ]

# The extension, fetched with the storage keys and checked against its pins (docker/sirius-fetch.sh).
FROM ubuntu:24.04 AS fetch
ARG AWS_CLI_VERSION
ARG AWS_CLI_SHA256
ARG SIRIUS_EXTENSION_S3
ARG SIRIUS_EXTENSION_SHA256
ARG SIRIUS_EXTENSION_BYTES
ARG SIRIUS_EXTENSION_UNPACKED_SHA256
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl unzip \
 && rm -rf /var/lib/apt/lists/* \
 && curl -fsSL -o /tmp/awscli.zip "https://awscli.amazonaws.com/awscli-exe-linux-x86_64-${AWS_CLI_VERSION}.zip" \
 && echo "${AWS_CLI_SHA256}  /tmp/awscli.zip" | sha256sum -c - \
 && unzip -q /tmp/awscli.zip -d /tmp && /tmp/aws/install -i /usr/local/aws-cli -b /usr/local/bin \
 && rm -rf /tmp/awscli.zip /tmp/aws \
 && aws --version | grep -q "^aws-cli/${AWS_CLI_VERSION} "
COPY docker/sirius-fetch.sh /tmp/sirius-fetch.sh
RUN --mount=type=secret,id=aws_access_key_id,required=true \
    --mount=type=secret,id=aws_secret_access_key,required=true \
    --mount=type=secret,id=aws_endpoint_url,required=true \
    --mount=type=secret,id=aws_region,required=true \
    bash /tmp/sirius-fetch.sh /run/secrets /out/sirius.duckdb_extension

FROM ubuntu:24.04
ARG SIRIUS_COMMIT
ARG SIRIUS_DUCKDB
ARG SIRIUS_LIBCUDF
ARG SIRIUS_EXTENSION_UNPACKED_SHA256
ARG SIRIUS_LICENSE_SHA256
ARG DRIVER_MINIMUM_SIRIUS
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
# The driver the extension's CUDA 13 needs, read by the device run before it starts anything.
LABEL peacockdb.driver-minimum=${DRIVER_MINIMUM_SIRIUS}
RUN userdel -r ubuntu && useradd -m -u 1000 -s /bin/bash peacock
COPY --from=python /opt/python /opt/python
COPY --from=python /opt/venv /opt/venv
COPY --from=fetch /out/sirius.duckdb_extension /opt/sirius/sirius.duckdb_extension
COPY docker/sirius-config.yaml /opt/sirius/sirius-config.yaml
COPY docker/sirius-bench.py /opt/sirius/sirius-bench.py
# The extension is Apache-2.0: Sirius's LICENSE at the pinned commit goes beside it.
COPY docker/sirius-LICENSE /opt/sirius/LICENSE
# What the image holds, for the runner's provenance line and its self-test.
RUN printf '%s\n' "sirius=${SIRIUS_COMMIT}" "duckdb=${SIRIUS_DUCKDB}" "libcudf=${SIRIUS_LIBCUDF}" \
      "extension_sha256=${SIRIUS_EXTENSION_UNPACKED_SHA256}" "license_sha256=${SIRIUS_LICENSE_SHA256}" > /opt/sirius/PIN \
 && grep -c '=.' /opt/sirius/PIN | grep -qx 5 \
 && echo "${SIRIUS_LICENSE_SHA256}  /opt/sirius/LICENSE" | sha256sum -c -
ENV PATH=/opt/venv/bin:$PATH SIRIUS_CONFIG_FILE=/opt/sirius/sirius-config.yaml
USER peacock
WORKDIR /tmp
