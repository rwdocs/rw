#!/usr/bin/env bash
# Egress firewall — adapted from anthropics/claude-code/.devcontainer/init-firewall.sh
# Allows: GitHub (CIDR ranges), npm registry, crates.io, Anthropic API, kroki.io,
# Playwright downloads, GitHub release artifacts, VS Code marketplace, telemetry
# (statsig/sentry — set CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 to skip those).
#
# Bootstrap dependency: api.github.com must be reachable while this script runs
# (we fetch meta-IP CIDRs before locking down the policy).

set -euo pipefail
IFS=$'\n\t'

# 1. Save Docker DNS NAT rules before flushing
DOCKER_DNS_RULES=$(iptables-save -t nat | grep "127\.0\.0\.11" || true)

# 2. Reset default policies to ACCEPT before flushing.
# `iptables -F` only flushes rules, not default policies. If a previous run set
# policies to DROP, the rest of this script (which needs to reach api.github.com
# to fetch CIDRs) would fail. Resetting first lets the script be re-run safely.
iptables -P INPUT ACCEPT
iptables -P FORWARD ACCEPT
iptables -P OUTPUT ACCEPT

# 3. Flush
iptables -F
iptables -X
iptables -t nat -F
iptables -t nat -X
iptables -t mangle -F
iptables -t mangle -X
ipset destroy allowed-domains 2>/dev/null || true

# 3. Restore Docker DNS
if [ -n "$DOCKER_DNS_RULES" ]; then
    echo "Restoring Docker DNS rules..."
    iptables -t nat -N DOCKER_OUTPUT 2>/dev/null || true
    iptables -t nat -N DOCKER_POSTROUTING 2>/dev/null || true
    echo "$DOCKER_DNS_RULES" | xargs -L 1 iptables -t nat
fi

# 4. Allow DNS, SSH, localhost — needed by the rest of this script
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
iptables -A INPUT -p udp --sport 53 -j ACCEPT
iptables -A OUTPUT -p tcp --dport 22 -j ACCEPT
iptables -A INPUT -p tcp --sport 22 -m state --state ESTABLISHED -j ACCEPT
iptables -A INPUT -i lo -j ACCEPT
iptables -A OUTPUT -o lo -j ACCEPT

# 5. Build allowed-domains ipset
ipset create allowed-domains hash:net

# 5a. GitHub IP ranges (web, api, git) — fetched from api.github.com/meta
echo "Fetching GitHub IP ranges..."
gh_ranges=$(curl -s https://api.github.com/meta)
if [ -z "$gh_ranges" ]; then
    echo "ERROR: Failed to fetch GitHub IP ranges"
    exit 1
fi
if ! echo "$gh_ranges" | jq -e '.web and .api and .git' >/dev/null; then
    echo "ERROR: GitHub API response missing required fields"
    exit 1
fi
while read -r cidr; do
    if [[ ! "$cidr" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}/[0-9]{1,2}$ ]]; then
        echo "ERROR: Invalid CIDR range from GitHub meta: $cidr"
        exit 1
    fi
    ipset add -exist allowed-domains "$cidr"
done < <(echo "$gh_ranges" | jq -r '(.web + .api + .git)[]' | aggregate -q)

# 5b. Hardcoded CDN anycast CIDRs.
# `dig` only captures the few IPs DNS returns at init time, but CDN-fronted services
# (crates.io subdomains on Fastly, kroki.io on Cloudflare proxy) anycast across large
# blocks and rotate which IPs DNS returns per request. Allowlist the published CDN
# ranges so requests to those services succeed regardless of which IP DNS hands out.
#
# Tradeoff: this also implicitly allowlists every other site on the same CDN. Users
# who need stricter isolation should switch to a hostname-aware proxy approach.
#
# Note on bare `crates.io`: it has migrated to AWS CloudFront and rotates across many
# CIDR ranges. We don't allowlist all of CloudFront. The actual cargo workflow uses
# `index.crates.io` (sparse index) and `static.crates.io` (crate downloads), both on
# Fastly's 151.101.0.0/16 and reachable. `cargo search` (which hits the website) does
# not work under firewall — disable firewall for that one command if needed.
echo "Adding CDN anycast ranges..."
for cidr in \
    "151.101.0.0/16" \
    "104.16.0.0/13" \
    "104.24.0.0/14" \
    "172.64.0.0/13"; do
    echo "  $cidr"
    ipset add -exist allowed-domains "$cidr"
done

# 5c. Resolve and add named domains (rw allowlist + Claude Code base).
# Kept for non-CDN services (api.anthropic.com, sentry.io, etc.) and as a safety
# net for CDN domains in case their published ranges expand.
for domain in \
    "registry.npmjs.org" \
    "crates.io" \
    "static.crates.io" \
    "index.crates.io" \
    "api.anthropic.com" \
    "statsig.anthropic.com" \
    "statsig.com" \
    "sentry.io" \
    "kroki.io" \
    "playwright.azureedge.net" \
    "cdn.playwright.dev" \
    "playwright.download.prss.microsoft.com" \
    "objects.githubusercontent.com" \
    "marketplace.visualstudio.com" \
    "vscode.blob.core.windows.net" \
    "update.code.visualstudio.com"; do
    echo "Resolving $domain..."
    ips=$(dig +noall +answer A "$domain" | awk '$4 == "A" {print $5}')
    if [ -z "$ips" ]; then
        echo "ERROR: Failed to resolve $domain"
        exit 1
    fi
    while read -r ip; do
        if [[ ! "$ip" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
            echo "ERROR: Invalid IP from DNS for $domain: $ip"
            exit 1
        fi
        ipset add -exist allowed-domains "$ip"
    done < <(echo "$ips")
done

# 6. Allow host's /24 network (so VS Code Server port forwarding works)
HOST_IP=$(ip route | grep default | cut -d" " -f3)
if [ -z "$HOST_IP" ]; then
    echo "ERROR: Failed to detect host IP"
    exit 1
fi
HOST_NETWORK=$(echo "$HOST_IP" | sed "s/\.[0-9]*$/.0\/24/")
echo "Host network detected as: $HOST_NETWORK"
iptables -A INPUT -s "$HOST_NETWORK" -j ACCEPT
iptables -A OUTPUT -d "$HOST_NETWORK" -j ACCEPT

# 7. Lock down: default DROP, allow established + allowed-domains, REJECT others
iptables -P INPUT DROP
iptables -P FORWARD DROP
iptables -P OUTPUT DROP
iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m set --match-set allowed-domains dst -j ACCEPT
iptables -A OUTPUT -j REJECT --reject-with icmp-admin-prohibited

# 8. Verify
echo "Verifying firewall rules..."
if curl --connect-timeout 5 https://example.com >/dev/null 2>&1; then
    echo "ERROR: Firewall verification failed - was able to reach https://example.com"
    exit 1
fi
echo "Firewall verification passed - example.com correctly blocked"
if ! curl --connect-timeout 5 https://api.github.com/zen >/dev/null 2>&1; then
    echo "ERROR: Firewall verification failed - unable to reach https://api.github.com"
    exit 1
fi
echo "Firewall verification passed - api.github.com correctly reachable"
echo "Firewall configuration complete"
