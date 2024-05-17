# pbddns - A simple Dynamic DNS (DDNS) client for Porkbun

`pbddns` is a Rust-based DDNS client intended to be run from a router,
interfacing with Porkbun's DNS service. This tool is designed to handle cases
where your network's public IP address changes frequently and needs to be
updated in your Porkbun domain's DNS records.

## Features
* Automatically detects public IP addresses from network interfaces
* Supports monitoring multiple network interfaces each with their own subdomain
* Avoids unnecessary updates
* Provides a simple status server for inspecting the current state of DNS records

## Installation
pbddns requires the Rust compiler to be built. Please follow the 
[Rust installation instructions](https://www.rust-lang.org/tools/install).

Once Rust is installed, clone the repository and build the binary using Cargo:

```bash
git clone https://github.com/dylanwh/pbddns.git
cd pbddns
cargo build --release
```

The compiled binary can be found under `./target/release/.`

## Configuration
Configuration of `pbddns` is achieved by command-line arguments.

#### Command-line arguments

* `-l, --listen <SocketAddr>`: Address to listen on for the status server (default: `0.0.0.0:3000`).
* `-d, --domain <String>`: The domain to update on Porkbun.
* `-i, --interface <InterfaceSubdomain>`: Mapping of network interface to subdomain. For
* example, `eth0=sub` or just `eth0` to use the root domain.
* `--ping`: Ping the Porkbun API to verify API credentials.
* `--once`: Only check interfaces and update dns once and exit
* `--write-pid <file>`: Write the process ID to the specified path. Useful for running pbddns as a service.

The `InterfaceSubdomain` structure is used for mapping network interfaces to subdomains.
It takes a string in the format `interface=subdomain.` If no subdomain is provided, the
updated DNS record will be the root domain itself.

#### Environment Variables
You must set your Porkbun API keys as environment variables for pbddns to work.
You can specify these in a .env file in the directory you run pbddns from
or provide them in one of the conventional ways for your operating system.

```bash
export PORKBUN_API_KEY="your-API-key" # porkbun calls this the "apikey".
export PORKBUN_SECRET_KEY="your-secret-key" # porkbun calls this the "secretapikey"
```

## Usage
To run pbddns, use the following command (example):

```bash
./pbddns -d yourdomain.com -i eth0 --ping
```

This command will update the `yourdomain.com` record on Porkbun to reflect the
public IP address of the `eth0` interface. The `--ping` option verifies the credentials
with Porkbun's API.

If you have multiple interfaces, you can specify them with multiple `-i' arguments:

```bash
./pbddns -d yourdomain.com -i eth0=first -i eth1=second --ping
```

This command will update the `first.yourdomain.com` record on Porkbun to reflect the
public IP address of the `eth0` interface and the `second.yourdomain.com` record on
Porkbun to remember the public IP address of the `eth1` interface.

The status server can be reached at `http://0.0.0.0:3000/` by default or at the `--listen` address if provided.

## License
This project is licensed under [MIT License](LICENSE).

## Disclaimer
This project is not affiliated with or endorsed by Porkbun LLC.
