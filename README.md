
# **Vecno Resolver**

**Vecno Resolver is a tool designed to resolve and manage configurations for private node clusters, particularly for the Vecno service. This document provides instructions for building, running, and deploying the resolver, as well as configuring a private node cluster.**

## **Prerequisites**

**Before building or running the Vecno Resolver, ensure you have the following installed:**

* [Rust](https://www.rust-lang.org/tools/install) (for building and running the resolver)
* [Cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) (Rust's package manager)
* **Any additional dependencies required for your Vecno cluster setup (e.g., networking tools, libraries)**

## **Building the Resolver**

**To build the Vecno Resolver, use the following command:**

```bash
cargo build --release --all
```

**Build Command Breakdown**

* **cargo build**: Compiles the project.
* **--release**: Builds the project in release mode for optimized performance.
* **--all**: Builds all packages in the workspace.

**This command generates the executable in the **target/release/** directory.**

**Running the Resolver for Testing**

**To test the Vecno Resolver locally, use the following command:**

```bash
cargo run --release -- --trace --verbose --config-file=examples/local.toml --auto-update
```

**Run Command Breakdown**

* **cargo run --release**: Runs the resolver in release mode.
* **--trace**: Enables trace-level logging for detailed debugging output.
* **--verbose**: Increases verbosity of the output for more detailed logs.
* **--config-file=examples/local.toml**: Specifies the configuration file to use (in this case, **local.toml** located in the **examples** directory).
* **--auto-update**: Enables automatic updates for the resolver.

**Ensure the **examples/local.toml** file exists in your project directory before running the command.**

**Deploying Under kHOST**

**To deploy the Vecno Resolver under a kHOST environment, follow these steps:**

1. **Copy the Configuration File**:
   Copy the **local.toml** configuration file to the **~/.vecno-resolver/** directory:

   ```bash
   cp examples/local.toml ~/.vecno-resolver/local.toml
   ```
2. **Configuration Override**:
   The presence of **~/.vecno-resolver/local.toml** will override any other configuration settings. Ensure the file is correctly configured for your deployment environment.

**Configuring a Private Node Cluster**

**To configure the Vecno Resolver for a private node cluster, use the provided **cluster.toml** example configuration. Follow these steps:**

1. **Copy the Cluster Configuration File**:
   Copy the **cluster.toml** file to the **~/.vecno-resolver/** directory:

   ```bash
   cp examples/cluster.toml ~/.vecno-resolver/cluster.toml
   ```
2. **Edit the Configuration**:
   Open **~/.vecno-resolver/cluster.toml** and modify it to match your cluster's actual configuration. Below is an example configuration:
   **toml**

   ```toml
   [[node]]
   service="vecno"
   transport-type="wrpc-borsh"
   tls=false
   network="mainnet"
   fqdn="127.0.0.1:8110"
   ```

   **Configuration Fields**

   * **service**: Specifies the service name (e.g., **vecno**).
   * **transport-type**: Defines the transport protocol (e.g., **wrpc-borsh**).
   * **tls**: Enables or disables TLS (set to **false** in the example).
   * **network**: Specifies the network (e.g., **mainnet**).
   * **fqdn**: The fully qualified domain name and port of the node (e.g., **127.0.0.1:8110**).

   **Add additional **[[node]]** sections for each node in your cluster as needed.**
3. **Apply the Configuration**:
   Ensure the resolver is configured to use **cluster.toml** if required, or rely on **local.toml** for kHOST deployments.

## **Notes**

* **The **local.toml** file takes precedence over other configuration files in kHOST deployments.**
* **For debugging, use the **--trace** and **--verbose** flags to generate detailed logs.**
* **Test your cluster configuration thoroughly in a non-production environment before deploying.**
* **If running multiple nodes, ensure each node’s **fqdn** and other settings are unique and correctly configured.**

**Contributing**

**Contributions to the Vecno Resolver are welcome! Please submit bug reports, feature requests, or pull requests via the project's repository.**
