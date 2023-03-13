# mu Protocol Litepaper

## Introduction

mu Protocol is a distributed, decentralized cloud provider marketplace. mu allows infrastructure providers to offer their servers for sale in a standardized and secure way. Apps designed for mu can work with any mu-enabled provider, which removes the vendor lock from most of today's cloud providers.

By creating a standardized way to provide and consume cloud technology, mu allows clients and providers to create a healthier and more competitive ecosystem, where apps can be easily developed and tested locally, and deployed with the same ease to the cloud. 

mu makes developers' lives considerably easier by providing a single, open standards-based and simple way to develop, test and deploy serverless applications. Developers need not worry anymore where they will be deploying their application, all they have to do is to implement it against the mu SDK and test it locally, where it will work **exactly the same way** as it would in the cloud. After the application is ready to be deployed, they simply choose a provider and a region from the mu Marketplace and deploy it. They will then pay per request, CPU and database use, just like they would with traditional cloud providers.

mu also simplifies cloud providers' lives. mu is not just a standardized way to write and deploy applications, it is also a standardized way to **provide cloud infrastructure**. This way, providers need not worry about which standards to be compatible with, which services to provide, etc. Instead, they simply run the mu executor, which will then turn their servers into mu nodes that can offer mu hosting services to the world. The mu executor is mostly self-contained and only needs a database cluster, no need to worry about Kubernetes or OpenShift anymore.

The mu runtime is based on the upcoming [WASI](https://wasi.dev) standard; an open, high performance system interface for WebAssembly.

mu consists of five main components:
* The mu smart contract
* The mu Marketplace
* The mu Executor
* The mu CLI
* The mu SDK

## The mu smart contract

The smart contract is the heart of the mu ecosystem, it implements the logic required for the Marketplace and for running tasks in the Executor. 

The smart contract allows providers to register as such and then add nodes to their regions. This way providers can be listed in the Marketplace and offer their cloud services to third parties. Providers must make a deposit in order to be listed, to guarantee only serious providers are part of the Marketplace.

The smart contract also allows clients to post their code to providers for execution. Clients have to make a prepayment to their escrow account (created using the CLI's `escrow create` command) to pay for usage. Then they add all the list of requested services and related information (known as a mu **stack**) in the transaction, such as environment variables, endpoints, etc.

After a stack is deployed to the network, provider nodes will periodically report usage to the smart contract, allowing it to charge the client for said usage and transfer it to the provider's account.

## The mu Marketplace

The mu Marketplace is where providers and clients interact with each other and with the network.

Providers can add nodes to a mu region, thus offering services on said region. The number of nodes and the exact specifications (CPU type, memory, network bandwidth, disk capacity, etc) of each node are private to the provider, who can choose what to disclose freely.

Clients can find which providers are available on the region they are interested in deploying to. After they have chosen a region and a provider, they can create a stack, make a prepayment to their escrow account and deploy it. Once the stack is deployed, mu will generate an endpoint address and will start accepting requests there.

### Provider registration

To register in the mu Marketplace, providers first need to use the mu CLI's `provider create` command to create their provider account in the smart contract. The provider deposit is also collected in this stage.

Provider accounts are created in disabled mode by default. To enable their account, providers must complete mu's KYC procedure, after which their account will be enabled and ready for use.

Once a provider's account is authorized, they can use the CLI's `provider region create` command to start creating their regions. To create a region, the following information must be provided:

* Region name
* Base URL
* Region number
* Minimum client escrow account balance
* Service pricing

The region's base URL is the URL at which the region's node can be reached from the internet. This URL must eventually resolve to the gateway port of at least one of the executor nodes within the cluster, though it is recommended to load-balance requests across all executor nodes in the region.

The region number is a numeric identifier used to distinguish the region from the provider's other regions and may be chosen freely.

The minimum client escrow account balance parameter is used to control when user stacks are disabled. Once a client's escrow account balance falls below this value, their stacks will be made unavailable and resources are no longer consumed for them. Usage is reported periodically and stack usage may go above the available balance; this value must be chosen in such a way that this situation will be avoided.

### Provider lists

Clients may retrieve lists of providers and their respective regions using the CLI's `list` command.

## The mu Executor

The mu Executor is the one in charge of registering nodes to the network, running stacks, routing incoming requests to the proper nodes, and storing data in the database.

Providers run the executor on their nodes in order to offer mu hosting services. The mu executor is designed to be as easy to deploy and maintain as possible. It is a single Rust binary requiring only a database cluster to run. No container, nor orchestrator, nor anything else is required. It can also run in a Docker container and/or a cluster orchestrator such as Kubernetes or Nomad.

The executor currently supports four services:

* mu Function Runtime
* mu Gateway
* muDB
* mu Storage

## The mu Function Runtime

This is where client code is executed. It is a WASI compatible runtime with a few extra features to better integrate with the mu ecosystem.

Nodes receive requests from the mu Gateway and execute the tasks on the mu Runtime. After the task has run, its output is sent back to the client through the Gateway.

Nodes load tasks on demand, after they receive the first request and then they cache them for some time. Nodes have a smart algorithm to choose which tasks need to remain cached and which ones can be purged.

### Communication with functions

The runtime uses STDIO pipes provided by WASI to communicate with functions. The runtime defines [certain messages](sdk/common/src/outgoing_message.rs) [that it recognizes](sdk/common/src/incoming_message.rs). These messages are serialized using [Borsh](https://borsh.io/) and written to standard input/output in binary format.

Note that while there currently exists only a rust SDK, any language with a WASI target can be used to read and process these messages and respond to the runtime.

### Billing

Code execution is billed in a unit called "megabyte-instruction", equivalent to one CPU instruction running with one megabyte of allocated memory. Since different providers use different hardware, billing by execution time will be unfair and prevent comparing provider prices. This is why usage is instead measured in number of instructions.

## The mu Gateway

The mu Gateway is the component that routes incoming requests to the runtime in order to process them. It is aware of the status of each stack and where it is deployed, and can route requests to the correct node for execution.

### Billing

Gateway requests are billed by the size of the request and response in bytes, and the number of requests processed by the cluster.

## muDB

muDB is the serverless database of the mu ecosystem. muDB is a simple and fast key/value store that can be accessed easily from inside the mu runtime. With it, it is possible to write complete stateful serverless applications, running entirely on mu.

### Billing

muDB usage is billed by number of read and write operations, with writes usually being more expensive than reads. If the `atomic` setting of muDB is used, each write will be billed as two.

Also, the overall database size is billed over time. This is not implemented yet in the demo version.

## mu Storage

mu Storage is the replicated and encrypted file storage service of the mu ecosystem. mu Storage allows data to be stored in boxes which can later be read either internally by mu functions or externally using HTTP.

### Billing

mu Storage is billed by volume of reads and writes, and also by overall size of stored objects over time. This is not implemented yet in the demo version.

## The mu CLI

The mu CLI contains a series of tools designed to simplify the development, testing and deployment of mu applications. The mu CLI is intended to be used by providers and developers targeting the mu platform.

## mu stacks

mu stacks are a collection of the service or services the clients want deployed, defined using YAML files known as stack manifests. The following is a simple stack manifest:

```yaml
name: GreetingApp
version: 0.1
services:
  - type: Function
    name: greeter_function
    binary: 8MdU7CQGkZaZPjPgXDwMoIkV
    runtime: wasi1.0
    memory_limit: 256MiB
    env: {}
  - type: Gateway
    name: greeting
    endpoints:
      /greet/{name}:
        - method: get
          route_to: greeter_function.greet_user
```

While stack manifests may be created manually, the recommended approach is to use the CLI's `init` command to create a mu project definition, and its `deploy` command to bundle the project and generate its stack manifest.

### muStack deployment and lifecycle

In order to deploy a stack, the first step is to select a provider from the provider list and choose one of their regions to deploy to (see above). Then the stack may be deployed and a prepayment made to the escrow account corresponding to the chosen provider.

Stacks are deployed by calling the `deployStack` RPC function on mu's smart contract. This function receives the stack manifest as well as the region ID.

Now, nodes in the selected region will receive this stack from the blockchain and start the deployment procedure.

The procedure is as follows:

1. Built function sources must be uploaded to the provider's mu Storage instance using the HTTP API beforehand. This is handled automatically by the CLI's `deploy` command.
2. The stack manifest is received from the blockchain, parsed and validated.
3. Every node will compare its own hash (made up of the node's IP, connection manager port and generation) to the stack's public key, calculating the Xor difference of the two IDs. This is called the distance to a stack.
4. Since nodes are aware of every other node on the cluster and their hashes via the membership table, the node which has the least distance to the stack will deploy it and be responsible for running the stack's functions. This information is also written to the membership table.
5. Other nodes periodically update their view of the membership table and will learn about the deployed stack, and will in turn route requests to that node for processing.

## The local runtime

The mu CLI comes with project management commands and a built-in local runtime for mu stacks.

Developers may use the `init` command to initialize a new project. Currently, only the Rust language is officially supported for the development of mu functions. Support for more languages will be added over time, but Rust will remain the recommended language due to its inherent safety and speed, thus reducing the carbon footprint and costs of running a mu stack.

Stacks may be run on the local runtime using the `run` command. This command will compile and deploy the stack and make its gateway endpoints available on a local port. Function logs will also be collected and printed to the terminal.

Note that to support this scenario, the CLI must come with bundled TiKV and JuiceFS binaries, which are quite large in size; therefore, CLI builds excluding the local runtime are made available for use by providers and on CI servers.

## Service billing

Nodes report to the network the service usage at regular intervals. They do so by calling the blockchain RPC function `updateUsage`. They include in this call each one of the services and their usage.

Calls to `updateUsage` must be made with a signature by the region's "authorized signer". For each region, an authorized signer must be created using the CLI's `provider signer create` command. This transaction will be signed by the provider's keypair, establishing a chain of trust that demonstrates that the provider (and not a malicious attacker) is billing a given service. The authorized signer is responsible for paying for the `updateUsage` smart contract call, so it must have SOL balance.

Finally the smart contract will calculate the cost by multiplying the provider's prices by the usage and then will take these tokens from the client's escrow account and transfer it to the provider's account.

# Deploying a mu region

To deploy a mu region, providers should first [deploy a TiKV cluster](https://tikv.org/docs/6.5/deploy/install/production/) using the TiUP tool. Each executor node must then be configured with the endpoints of the TiKV cluster, the provider's public key, and the region number. Nodes will automatically discover other nodes in the cluster via a membership table mechanism that uses the TiKV cluster as a synchronization back-end. Note that the TiKV cluster is also the backing storage for all muDB instances used by clients' stacks, so its capacity must be planned accordingly.

The executor also requires an S3-compatible object storage to back the mu Storage service. The executor itself comes with an embedded [JuiceFS](https://juicefs.com) binary which may be configured to use the same TiKV cluster as its storage back-end; however, this setup is only meant for quick deployment of mu clusters for testing and is not recommended. Any storage system with an S3-compatible API may be used. Only one bucket is needed, and the same bucket should be used by all nodes in a cluster.