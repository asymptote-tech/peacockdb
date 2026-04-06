#!/bin/bash

# Startup script to run on a fresh Fedora 43 VM as root.

if [ -z "$USER" ]; then
  USER="build"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Create user and set up passwordless sudo
useradd -m -s /bin/bash "$USER"
echo "$USER ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Copy SSH authorized_keys from root and set correct permissions
mkdir -p "/home/$USER/.ssh"
cp /root/.ssh/authorized_keys "/home/$USER/.ssh/"
chown -R "$USER:$USER" "/home/$USER/.ssh"
chmod 700 "/home/$USER/.ssh"
chmod 600 "/home/$USER/.ssh/authorized_keys"

# Update package list and install system packages
dnf install -y dnf-plugins-core   # for config-manager if needed later
dnf update -y

dnf install -y \
    curl \
    unzip \
    patchelf \
    openssl-devel \
    zlib-devel \
    boost-devel \
    git \
    pkg-config \
    python3-pip \
    python3-devel \
    gcc \
    gcc-c++

# Install CUDA toolkit 12.6 from NVIDIA's official repository
# Add NVIDIA CUDA repository for Fedora 43
dnf config-manager --add-repo https://developer.download.nvidia.com/compute/cuda/repos/fedora43/x86_64/cuda-fedora43.repo
dnf clean all
dnf install -y cuda-toolkit-13-2

# Install CMake 4.2.3 from the official tarball
CMAKE_VERSION=4.2.3
curl -fsSL "https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz" \
  | tar -xz -C /usr/local --strip-components=1

# Install Ninja from the GitHub release
NINJA_VERSION=1.12.1
curl -fsSL "https://github.com/ninja-build/ninja/releases/download/v${NINJA_VERSION}/ninja-linux.zip" \
  -o /tmp/ninja-linux.zip
unzip -o /tmp/ninja-linux.zip -d /usr/local/bin
rm /tmp/ninja-linux.zip
chmod +x /usr/local/bin/ninja

echo "$SCRIPT_DIR/conda-install-cudf.sh" --user "$USER"
# Run the conda installation script (assumed to be in the same directory)
"$SCRIPT_DIR/conda-install-cudf.sh" --user "$USER"