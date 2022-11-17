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

## The Mu Executor

The Mu Executor is the one in charge of registering nodes to the network, running jobs, routing incoming requests to the proper nodes, and storing data in the database.

Infrastructure providers run the Executor on their nodes in order to offer Mu hosting services. The Mu executor is designed to be as easy to deploy and maintain as possible. It is a single, self-contained Rust binary. No container, nor orchestator, nor anything else is required. It runs on Linux, Windows and MacOS. It _can_ run of course in a Docker container and/or a cluster orchestator such as Kubernetes or Nomad.

Running the executor on a server is extremely easy, all it is needed to provide is the node key, that was previously generated by the Marketplace and signed by the provider account.

The executor currently supports three services:

* The Mu Runtime
* MuGateway
* MuDB

### The Mu Runtime

This is where the jobs are executed. It is a WASI compatible runtime with a few extra features to better integrate with the Mu ecosystem.

Nodes receive requests from the Mu Gateway and execute the tasks on the Mu Runtime. After the task has run, its output is sent back to the client through the Gateway.

Nodes load tasks on demand, after they receive the first request and then they cache them for some time. Nodes have a smart algorithm to choose which tasks need to remain cached and which ones can be purged.

### The Mu Gateway

The Mu Gateway is the component that routes incoming requests to nodes in order to process them. It is smart enough to remember which nodes have recently executed which tasks and tries to keep sending the same tasks to the same nodes. It also works as a load balancer and tries to share the load evenly and smartly across the nodes.

### MuDB

MuDB is the serverless database of the Mu ecosystem. MuDB is a simple and fast, key/value store that can be accessed easily from inside the Mu runtime. With this it is possible to write complete, stateful serverless applications, running entirely on Mu.

The runtime implements a special endpoint 

## The Mu Toolkit 

The Mu toolkit is a series of tools designed to simplify the development, testing and deployment of Mu applications. The Mu toolkit is intended to be used by developers targeting the Mu platform.

The Mu toolkit includes the `mu` command, that contains a series of subcommands, useful for developers.
