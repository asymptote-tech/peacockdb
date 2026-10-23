# check=skip=InvalidDefaultArgInFrom
# The run image for one cuDF version: the runtime of the environment the binaries were built
# against, and nothing to build with. Built by scripts/env/image.sh, which passes every
# docker/pins.env line as a build arg; its use and size are in
# llm-wiki/build-test.md, "Images". The binaries are mounted at /peacock/install and the data at
# /peacock/testdata (docker/run-tests.sh). The CUDA driver is the host's, mounted by the NVIDIA
# Container Toolkit under `docker run --gpus all`. No secret, host or user home is in any layer.
# No default: without --build-arg CUDF the last FROM names no stage and the build stops (line 1).
ARG CUDF

# /opt/cudf is the build image's prefix less what only a build or Python needs
# (docker/runtime-spec.sh), at the same path. The download tools stay in this stage, and so does
# nothing else: the package cache and the static archives runtime packages ship too
# (libcuvs_static.a alone is 1 GB) go in the same step.
FROM ubuntu:24.04 AS prefix
ARG CUDF
ARG MICROMAMBA_VERSION
ARG MICROMAMBA_SHA256
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
COPY docker/conda/ /tmp/conda/
COPY docker/runtime-spec.sh /tmp/runtime-spec.sh
RUN /tmp/runtime-spec.sh "/tmp/conda/cudf-${CUDF}.spec" > /tmp/runtime.spec \
 && curl -fsSL -o /tmp/micromamba \
      "https://github.com/mamba-org/micromamba-releases/releases/download/${MICROMAMBA_VERSION}/micromamba-linux-64" \
 && echo "${MICROMAMBA_SHA256}  /tmp/micromamba" | sha256sum -c - \
 && chmod +x /tmp/micromamba \
 && MAMBA_ROOT_PREFIX=/tmp/mamba /tmp/micromamba create -y --no-rc -p /opt/cudf -f /tmp/runtime.spec \
 && [ "$(ls /opt/cudf/conda-meta/*.json | wc -l)" -eq "$(grep -c '^https://' /tmp/runtime.spec)" ] \
 && [ -f /opt/cudf/lib/libcudf.so ] && [ ! -e /opt/cudf/bin/python ] \
 && find /opt/cudf -name '*.a' -delete \
 && rm -rf /tmp/mamba /tmp/micromamba /tmp/runtime.spec /tmp/conda

# The driver each version's binaries need, read by the device run before it ships anything: the
# toolkit driver of the spec's cudart (pins.env). The stage is chosen by version, as the build
# image chooses its gcc.
FROM ubuntu:24.04 AS run-25.02
ARG DRIVER_MINIMUM_25_02
RUN [ -n "${DRIVER_MINIMUM_25_02}" ]
LABEL peacockdb.driver-minimum=${DRIVER_MINIMUM_25_02}

FROM ubuntu:24.04 AS run-26.02
ARG DRIVER_MINIMUM_26_02
RUN [ -n "${DRIVER_MINIMUM_26_02}" ]
LABEL peacockdb.driver-minimum=${DRIVER_MINIMUM_26_02}

FROM run-${CUDF}
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
RUN userdel -r ubuntu && useradd -m -u 1000 -s /bin/bash peacock
COPY --from=prefix /opt/cudf /opt/cudf
COPY docker/run-tests.sh /opt/run/run-tests.sh
# The default user, so the image never runs as root unasked; `docker run --user` gives the
# machine user's uid instead, and nothing here is found through HOME.
USER peacock
WORKDIR /tmp
ENTRYPOINT ["/opt/run/run-tests.sh"]
