ARG VARIANT=ubuntu-24.04
FROM mcr.microsoft.com/devcontainers/base:${VARIANT}

# System deps: napi-rs build, firewall tooling, devex.
# Playwright system deps are NOT installed here — `npx playwright install --with-deps`
# in post-create.sh installs the canonical set per Playwright version.
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config libssl-dev \
    iptables ipset aggregate dnsutils jq \
    git curl ca-certificates zsh fzf less \
    && rm -rf /var/lib/apt/lists/*

# Install helper scripts and grant passwordless sudo for them.
# This is additive on top of the base image's general sudo for `vscode`.
COPY init-firewall.sh /usr/local/bin/init-firewall.sh
COPY devcontainer-prepare.sh /usr/local/bin/devcontainer-prepare.sh
RUN chmod +x /usr/local/bin/init-firewall.sh /usr/local/bin/devcontainer-prepare.sh \
    && echo "vscode ALL=(root) NOPASSWD: /usr/local/bin/init-firewall.sh, /usr/local/bin/devcontainer-prepare.sh" \
       > /etc/sudoers.d/devcontainer-helpers \
    && chmod 0440 /etc/sudoers.d/devcontainer-helpers

USER vscode
ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH
