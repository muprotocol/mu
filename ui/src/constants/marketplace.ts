export const marketplace = {
    "version": "0.1.0",
    "name": "marketplace",
    "instructions": [
    {
        "name": "initialize",
        "accounts": [
            {
                "name": "state",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "mint",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "depositToken",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "authority",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "tokenProgram",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": []
    },
    {
        "name": "createProviderAuthorizer",
        "accounts": [
            {
                "name": "state",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "providerAuthorizer",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "authority",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "authorizer",
                "isMut": false,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": []
    },
    {
        "name": "createProvider",
        "accounts": [
            {
                "name": "state",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "provider",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "depositToken",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "owner",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "ownerToken",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "tokenProgram",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "rent",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": [
            {
                "name": "name",
                "type": "string"
            }
        ]
    },
    {
        "name": "authorizeProvider",
        "accounts": [
            {
                "name": "providerAuthorizer",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "authorizer",
                "isMut": false,
                "isSigner": true
            },
            {
                "name": "provider",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "owner",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": []
    },
    {
        "name": "createRegion",
        "accounts": [
            {
                "name": "provider",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "region",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "owner",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": [
            {
                "name": "regionNum",
                "type": "u32"
            },
            {
                "name": "name",
                "type": "string"
            },
            {
                "name": "zones",
                "type": "u8"
            },
            {
                "name": "rates",
                "type": {
                    "defined": "ServiceRates"
                }
            }
        ]
    },
    {
        "name": "createStack",
        "accounts": [
            {
                "name": "provider",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "region",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "stack",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "user",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": [
            {
                "name": "stackSeed",
                "type": "u64"
            },
            {
                "name": "stackData",
                "type": "bytes"
            },
            {
                "name": "name",
                "type": "string"
            }
        ]
    },
    {
        "name": "createAuthorizedUsageSigner",
        "accounts": [
            {
                "name": "provider",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "region",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "authorizedSigner",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "owner",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": [
            {
                "name": "signer",
                "type": "publicKey"
            },
            {
                "name": "tokenAccount",
                "type": "publicKey"
            }
        ]
    },
    {
        "name": "createProviderEscrowAccount",
        "accounts": [
            {
                "name": "state",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "mint",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "escrowAccount",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "provider",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "user",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "tokenProgram",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "rent",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": []
    },
    {
        "name": "updateUsage",
        "accounts": [
            {
                "name": "state",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "authorizedSigner",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "region",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "tokenAccount",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "usageUpdate",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "escrowAccount",
                "isMut": true,
                "isSigner": false
            },
            {
                "name": "stack",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "signer",
                "isMut": true,
                "isSigner": true
            },
            {
                "name": "systemProgram",
                "isMut": false,
                "isSigner": false
            },
            {
                "name": "tokenProgram",
                "isMut": false,
                "isSigner": false
            }
        ],
        "args": [
            {
                "name": "updateSeed",
                "type": "u128"
            },
            {
                "name": "escrowBump",
                "type": "u8"
            },
            {
                "name": "usage",
                "type": {
                    "defined": "ServiceUsage"
                }
            }
        ]
    }
],
    "accounts": [
    {
        "name": "MuState",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "authority",
                    "type": "publicKey"
                },
                {
                    "name": "mint",
                    "type": "publicKey"
                },
                {
                    "name": "depositToken",
                    "type": "publicKey"
                },
                {
                    "name": "bump",
                    "type": "u8"
                }
            ]
        }
    },
    {
        "name": "ProviderAuthorizer",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "authorizer",
                    "type": "publicKey"
                },
                {
                    "name": "bump",
                    "type": "u8"
                }
            ]
        }
    },
    {
        "name": "Provider",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "owner",
                    "type": "publicKey"
                },
                {
                    "name": "authorized",
                    "type": "bool"
                },
                {
                    "name": "name",
                    "type": "string"
                },
                {
                    "name": "bump",
                    "type": "u8"
                }
            ]
        }
    },
    {
        "name": "ProviderRegion",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "provider",
                    "type": "publicKey"
                },
                {
                    "name": "zones",
                    "type": "u8"
                },
                {
                    "name": "regionNum",
                    "type": "u32"
                },
                {
                    "name": "rates",
                    "type": {
                        "defined": "ServiceRates"
                    }
                },
                {
                    "name": "bump",
                    "type": "u8"
                },
                {
                    "name": "name",
                    "type": "string"
                }
            ]
        }
    },
    {
        "name": "Stack",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "user",
                    "type": "publicKey"
                },
                {
                    "name": "region",
                    "type": "publicKey"
                },
                {
                    "name": "seed",
                    "type": "u64"
                },
                {
                    "name": "revision",
                    "type": "u32"
                },
                {
                    "name": "bump",
                    "type": "u8"
                },
                {
                    "name": "name",
                    "type": "string"
                },
                {
                    "name": "stack",
                    "type": "bytes"
                }
            ]
        }
    },
    {
        "name": "AuthorizedUsageSigner",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "signer",
                    "type": "publicKey"
                },
                {
                    "name": "tokenAccount",
                    "type": "publicKey"
                }
            ]
        }
    },
    {
        "name": "UsageUpdate",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "accountType",
                    "type": "u8"
                },
                {
                    "name": "region",
                    "type": "publicKey"
                },
                {
                    "name": "stack",
                    "type": "publicKey"
                },
                {
                    "name": "seed",
                    "type": "u128"
                },
                {
                    "name": "usage",
                    "type": {
                        "defined": "ServiceUsage"
                    }
                }
            ]
        }
    }
],
    "types": [
    {
        "name": "ServiceRates",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "billionFunctionMbInstructions",
                    "type": "u64"
                },
                {
                    "name": "dbGigabyteMonths",
                    "type": "u64"
                },
                {
                    "name": "millionDbReads",
                    "type": "u64"
                },
                {
                    "name": "millionDbWrites",
                    "type": "u64"
                },
                {
                    "name": "millionGatewayRequests",
                    "type": "u64"
                },
                {
                    "name": "gigabytesGatewayTraffic",
                    "type": "u64"
                }
            ]
        }
    },
    {
        "name": "ServiceUsage",
        "type": {
            "kind": "struct",
            "fields": [
                {
                    "name": "functionMbInstructions",
                    "type": "u128"
                },
                {
                    "name": "dbBytesSeconds",
                    "type": "u128"
                },
                {
                    "name": "dbReads",
                    "type": "u64"
                },
                {
                    "name": "dbWrites",
                    "type": "u64"
                },
                {
                    "name": "gatewayRequests",
                    "type": "u64"
                },
                {
                    "name": "gatewayTrafficBytes",
                    "type": "u64"
                }
            ]
        }
    },
    {
        "name": "MuAccountType",
        "type": {
            "kind": "enum",
            "variants": [
                {
                    "name": "MuState"
                },
                {
                    "name": "Provider"
                },
                {
                    "name": "ProviderRegion"
                },
                {
                    "name": "UsageUpdate"
                },
                {
                    "name": "AuthorizedUsageSigner"
                },
                {
                    "name": "Stack"
                },
                {
                    "name": "ProviderAuthorizer"
                }
            ]
        }
    }
],
    "errors": [
    {
        "code": 6000,
        "name": "ProviderNotAuthorized",
        "msg": "Provider is not authorized"
    }
],
    "metadata": {
    "address": "2MZLka8nfoAf1LKCCbgCw5ZXfpMbKGDuLjQ88MNMyti2"
}
}