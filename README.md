# Mu Protocol Whitepaper

## Introduction

MuProtocol is a distributed, decentralized cloud provider marketplace. Mu allows infrastructure providers to offer their servers for sale in a standardized and secure way. Apps designed for Mu can work with any Mu-enabled provider, which removes the vendor lock from most today's cloud providers.

By creating a standardized way to provide and consume cloud technology, Mu allows clients and providers to create a healthier and more competitive ecosystem, where apps can be easily developed and tested locally, and deployed with the same ease to the cloud. 

Mu makes developers' life considerably easier by providing a single, open standards-based and simple way to develop, test and deploy Serverless applications. Developers need not to worry anymore where they will be deploying their application, all they have to do is to write it with the Mu toolkit and test it locally, where it will work **exactly the same way** as it would in the cloud. After the application is ready to be deployed, they simply choose a provider and a region from the Mu Marketplace and deploy it. They will then pay per request, CPU and database use, just like they would with traditional cloud providers.

Mu also simplifies the cloud providers' life. Mu is not just a standardized way to write and deploy applications, it is also a standardized way to **provide cloud infrastructure**. This way, providers need not to worry about which cloud to be compatible with, which services to provide, etc, instead, they simply run the Mu executor, Mu will then turn their nodes into Mu nodes that can already offer Mu hosting services to the world. The Mu executor is self-contained and has no external dependencies, no need to worry about Kubernetes or OpenShift anymore.

The Mu runtime is based in the upcoming [WASI](https://wasi.dev) standard, an open, high performance system interface for WebAssembly.

Mu consists in four main components:
* The Mu smart contract
* The Mu Marketplace
* The Mu Executor
* The Mu Toolkit

## The Mu smart contract

The smart contract is the heart of the Mu ecosystem, it implements the logic required for the Marketplace and for running tasks in the Executor. 

The smart contract allows providers to register as such and then add nodes to  Mu regions. This way providers can be listed in the Marketplace and offer their cloud services to third parties. Providers must make a deposit in order to be listed, to guarantee only serious providers are part of the Marketplace.

The smart contract also allows clients to post their code to providers for execution. Clients have to make a prepayment to pay for the usage. Then they add all the job-related information in the transaction, such as environment variables, endpoints, etc. This information is encrypted using the provider's public key, so that only said provider can access this information, as it may contain secrets and/or other sensitive information.

After a job is deployed to the network, provider nodes will periodically report usage to the smart contract, allowing it to charge the client for said usage and transfer it to the provider's account.

All payments in the network are done using a stable coin or token that will be chosen by the community.

## The Mu Marketplace

The Mu Marketplace is where providers and clients interact with each other and with the network.

Providers can add nodes to a Mu region, thus offering services on said region. The number of nodes and the exact specifications (CPU type, memory, network bandwidth, disk capacity, etc) of each node are private to the provider, who can choose what to disclose freely.

Clients can find which providers are available on the region they are interested in deploying to. After they have chosen a region and a provider, they can create a job, make a prepayment and deploy it. Once the job is deployed, Mu will generate an endpoint address and will start accepting requests there.

### Provider registration

### Running a Mu Node
- node key

### Provider lists

## The Mu Executor

The Mu Executor is the one in charge of registering nodes to the network, running jobs, routing incoming requests to the proper nodes, and storing data in the database.

Infrastructure providers run the Executor on their nodes in order to offer Mu hosting services. The Mu executor is designed to be as easy to deploy and maintain as possible. It is a single, self-contained Rust binary. No container, nor orchestator, nor anything else is required. It runs on Linux, Windows and MacOS. It _can_ run of course in a Docker container and/or a cluster orchestator such as Kubernetes or Nomad.

Running the executor on a server is extremely easy, all it is needed to provide is the node key, that was previously generated by the Marketplace and signed by the provider account.

The executor currently supports four services:

* The Mu Runtime
* MuGateway
* MuDB
* MuStore

## The Gossip protocol

## The Mu Runtime

This is where the jobs are executed. It is a WASI compatible runtime with a few extra features to better integrate with the Mu ecosystem.

Nodes receive requests from the Mu Gateway and execute the tasks on the Mu Runtime. After the task has run, its output is sent back to the client through the Gateway.

Nodes load tasks on demand, after they receive the first request and then they cache them for some time. Nodes have a smart algorithm to choose which tasks need to remain cached and which ones can be purged.

### Parameter passing

### Billing

## The Mu Gateway

The Mu Gateway is the component that routes incoming requests to nodes in order to process them. It is smart enough to remember which nodes have recently executed which tasks and tries to keep sending the same tasks to the same nodes. It also works as a load balancer and tries to share the load evenly and smartly across the nodes.

## MuDB

MuDB is the serverless database of the Mu ecosystem. MuDB is a simple and fast, key/value store that can be accessed easily from inside the Mu runtime. With this it is possible to write complete, stateful serverless applications, running entirely on Mu.

### Billing

## MuStore

MuStore is the replicated and encrypted file storage service of the Mu Ecosystem. MuStore allows data to be stored in "boxes" which can be later read either internally by MuFunctions or externally using HTTP.

### Billing

## The Mu Toolkit 

The Mu toolkit is a series of tools designed to simplify the development, testing and deployment of Mu applications. The Mu toolkit is intended to be used by developers targetting the Mu platform.

The Mu toolkit includes the `mu` command, that contains a series of subcommands, useful for developers.

## MuStacks

MuStacks are YAML files that contain the description of the service or services that want to be deployed and how they connect to each other. For example a stack can define multiple endpoints linked to different functions.

Here is a sample Stack:

```yaml
name: SimpleApp
version: 0.1
services:
  - type: MuFunction
    name: simple_backend
    binary: https://storage-service/code.tar.gz
    runtime: wasi1.0
    env:
      - VAR1: value1
      - VAR2: value2
  - type: MuGateway
    name: main_gw
    endpoints:
      /login:
        type: POST
        route_to: simple_backend
  - type: MuDb
    name: main_db
```

Services can be either a MuFunction, a MuDb table or a MuGateway endpoint.

### MuStack deployment and lifecycle

In order to deploy a stack, the first step is to select a provider from the provider list (see above). Then we can deploy the Stack and make a prepayment to run said Stack using Mu's smart contract. 

The stack is deployed by calling an Rpc function on Mu's smart contract (`deployStack`), this function receives the stack yaml file as well as the provider ID and the zone ID, plus the amount of tokens that would like to be prepaid for this service. Afterwards, the data is stored in the blockchain and the tokens are locked by the contract.

Now, the nodes that match the selection criteria (provider and region) will receive this stack from the blockchain and start the deployment procedure.

The procedure is as follows:

1. The entire YAML file containing the stack gets hashed together with the block where it was committed and the network clock. This is called the stack hash. 
2. Every service gets hashed by hashing its name concatenated with the stack hash. This is called the service hash.
3. Every node will now compare its own hash to the service hash, calculating the Xor difference of the two IDs. This is called the distance to a service.
4. Since nodes are aware of every other node on the cluster (thanks to the Gossip protocol) and their IDs, nodes will then select the node (or nodes if the service require more than one instance) with the lowest distance to the service.
5. The nodes with the lowest distance to the service will then run the service and update the Gossip state accordingly.

### Service billing

Nodes report to the network the service usage at regular intervals. They do so by calling the blockchain Rpc function `updateUsage`. They include in this call each one of the services and their usage.

The usage units and billing mechanism varies from service to service. Some services are billed by usage units (for example functions are billed by CPU-seconds), other services are billed by space and time (for example MuStore is billed by GB/month), etc. Services may even be billed by more than one paramater, for example, MuDB is billed by storage usage (in GB/month) and by number of requests.

`updateUsage` requires a struct with each of the services usage plus the node's certificate. Also it will take the signature of the usage struct, signed by the node's private key.

The smart contract will receive this data and will check that the node's certificate is signed by the provider's certificate. Moreover, it will check that the billing struct is signed by the node. This way a chain of trust is built that demonstrates that the provider (and not a malicious attacker) is billing a given service (or services).

Finally the smart contract will calculate the cost by multiplying the provider's prices by the usage and then will take these tokens from the prepayment and transfer it to the provider's account.

Just a litttle word that has problem in spellss :)
