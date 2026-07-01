#!/usr/bin/env node
import fs from "node:fs"
import path from "node:path"
import process from "node:process"

const root = process.cwd()
const frontendContractPath =
  process.env.FRONTEND_CONTRACT_PATH ??
  path.resolve(root, "../avato-wallet-frontend/contracts/rgb-on-tondi-http-contract.json")
const rustContractPath = path.resolve(root, "rgb/cli/src/http_contract.rs")

const frontend = JSON.parse(fs.readFileSync(frontendContractPath, "utf8"))
const rustSource = fs.readFileSync(rustContractPath, "utf8")
const failures = []
const requestTypeNames = [
  "CreateWalletInput",
  "UnlockWalletInput",
  "ImportIssuerInput",
  "IssueContractInput",
  "AcceptContractInput",
  "CreateInvoiceInput",
  "PayInvoiceInput",
  "PayInvoicesInput",
  "AcceptTransferInput",
  "FinaliseTransferInput",
]
const allowedSchemaTypes = new Set([
  "string",
  "number",
  "boolean",
  "string[]",
  "TondiNetwork",
])

function fail(message) {
  failures.push(message)
}

function assert(condition, message) {
  if (!condition) fail(message)
}

function requestName(endpoint) {
  if (endpoint.request === undefined) return null
  if (typeof endpoint.request === "string") return endpoint.request
  const request = endpoint.request
  if (request?.passphrase === "string") return "UnlockWalletInput"
  if (request?.fileName === "string") return "ImportIssuerInput"
  if (request?.consignmentName === "string") return "AcceptContractInput"
  if (request?.consignmentRef === "string") return "AcceptTransferInput"
  if (request?.psttRef === "string" && request?.broadcast === "boolean") {
    return "FinaliseTransferInput"
  }
  return JSON.stringify(request)
}

function responseName(endpoint) {
  if (endpoint.response === null) return "Empty"
  if (typeof endpoint.response === "string") {
    if (endpoint.notFoundResponse === null) {
      return `NotFoundNullJson:${endpoint.response}`
    }
    if (endpoint.nullableResponse === true) {
      return `NullableJson:${endpoint.response}`
    }
    return `Json:${endpoint.response}`
  }
  if (endpoint.response?.address === "string") return "AddressString"
  return JSON.stringify(endpoint.response)
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function snakeToCamel(value) {
  return value.replace(/_([a-z])/g, (_, char) => char.toUpperCase())
}

function rustTypeName(value) {
  const compact = value.replace(/\s+/g, "")
  if (compact === "String") return "string"
  if (compact === "bool") return "boolean"
  if (compact === "u8" || compact === "u16" || compact === "u32" || compact === "usize") {
    return "number"
  }
  if (compact === "Vec<String>") return "string[]"
  if (compact === "TondiNetwork") return "TondiNetwork"
  return compact
}

function rustStructFields(source, typeName) {
  const match = new RegExp(`pub\\s+struct\\s+${typeName}\\s*\\{([\\s\\S]*?)\\n\\}`).exec(
    source,
  )
  if (!match) return null

  const fields = {}
  for (const field of match[1].matchAll(/pub\s+([a-z][a-z0-9_]*):\s*([^,\n]+),/g)) {
    const [, rustName, rustType] = field
    fields[snakeToCamel(rustName)] = rustTypeName(rustType)
  }
  return fields
}

function assertExactSchema(actual, expected, label) {
  const actualKeys = Object.keys(actual).sort()
  const expectedKeys = Object.keys(expected).sort()
  assert(
    actualKeys.join(",") === expectedKeys.join(","),
    `${label} keys ${actualKeys.join(",")} must equal ${expectedKeys.join(",")}`,
  )
  for (const key of expectedKeys) {
    assert(
      actual[key] === expected[key],
      `${label}.${key} type ${actual[key]} must equal ${expected[key]}`,
    )
  }
}

function endpointKey(endpoint) {
  return `${endpoint.method} ${endpoint.path}`
}

function frontendEndpointMap(contract) {
  const result = new Map()
  for (const endpoint of contract.endpoints ?? []) {
    result.set(endpointKey(endpoint), {
      request: requestName(endpoint),
      response: responseName(endpoint),
    })
  }
  return result
}

function rustEndpointMap(source) {
  const result = new Map()
  const endpointPattern =
    /Endpoint\s*\{\s*method:\s*HttpMethod::(Get|Post),\s*path:\s*"([^"]+)",\s*request:\s*(None|Some\("([^"]+)"\)),\s*response:\s*ResponseKind::([A-Za-z]+)(?:\("([^"]+)"\))?,\s*\}/gs

  for (const match of source.matchAll(endpointPattern)) {
    const [, method, endpointPath, requestExpr, requestType, responseKind, responseType] = match
    const httpMethod = method === "Get" ? "GET" : "POST"
    let response
    switch (responseKind) {
      case "Json":
        response = `Json:${responseType}`
        break
      case "NullableJson":
        response = `NullableJson:${responseType}`
        break
      case "NotFoundNullJson":
        response = `NotFoundNullJson:${responseType}`
        break
      case "AddressString":
      case "Empty":
        response = responseKind
        break
      default:
        response = `${responseKind}:${responseType ?? ""}`
    }
    result.set(`${httpMethod} ${endpointPath}`, {
      request: requestExpr === "None" ? null : requestType,
      response,
    })
  }
  return result
}

assert(frontend.version === 1, "frontend contract version must be 1")
assert(Array.isArray(frontend.endpoints), "frontend contract must list endpoints")
assert(isRecord(frontend.requestTypes), "frontend contract must define requestTypes")
assert(
  !frontend.enums?.dashboardServiceMode?.includes("dev-preview"),
  "frontend production backend serviceMode enum must exclude dev-preview",
)

for (const typeName of requestTypeNames) {
  const schema = frontend.requestTypes?.[typeName]
  assert(isRecord(schema), `frontend contract missing request type ${typeName}`)
  if (isRecord(schema)) {
    for (const [fieldName, fieldType] of Object.entries(schema)) {
      assert(
        fieldName !== "tondiP2mrSecretKeys",
        `${typeName} must not expose raw P2MR signer material`,
      )
      assert(
        typeof fieldType === "string" && allowedSchemaTypes.has(fieldType),
        `${typeName}.${fieldName} has unsupported schema type ${fieldType}`,
      )
    }
  }

  const rustFields = rustStructFields(rustSource, typeName)
  assert(rustFields !== null, `Rust contract missing DTO ${typeName}`)
  if (rustFields && isRecord(schema)) assertExactSchema(rustFields, schema, typeName)
}

const frontendEndpoints = frontendEndpointMap(frontend)
const rustEndpoints = rustEndpointMap(rustSource)

assert(
  rustEndpoints.size === frontendEndpoints.size,
  `Rust endpoint count ${rustEndpoints.size} must equal frontend endpoint count ${frontendEndpoints.size}`,
)

for (const [key, frontendSpec] of frontendEndpoints) {
  const rustSpec = rustEndpoints.get(key)
  assert(rustSpec !== undefined, `Rust contract missing endpoint ${key}`)
  if (!rustSpec) continue
  assert(
    rustSpec.request === frontendSpec.request,
    `${key} request mismatch: Rust ${rustSpec.request} vs frontend ${frontendSpec.request}`,
  )
  assert(
    rustSpec.response === frontendSpec.response,
    `${key} response mismatch: Rust ${rustSpec.response} vs frontend ${frontendSpec.response}`,
  )
}

for (const key of rustEndpoints.keys()) {
  assert(frontendEndpoints.has(key), `frontend contract missing Rust endpoint ${key}`)
}

const requiredRustTypes = [
  "WalletProfile",
  "P2mrOutput",
  "RgbAsset",
  "RgbInvoice",
  "RgbTransfer",
  "DashboardSnapshot",
  "CreateWalletInput",
  "UnlockWalletInput",
  "ImportIssuerInput",
  "IssueContractInput",
  "AcceptContractInput",
  "CreateInvoiceInput",
  "PayInvoiceInput",
  "PayInvoicesInput",
  "AcceptTransferInput",
  "FinaliseTransferInput",
  "P2mrAddressResponse",
]

for (const typeName of requiredRustTypes) {
  assert(
    new RegExp(`struct\\s+${typeName}\\b`).test(rustSource),
    `Rust contract missing DTO ${typeName}`,
  )
}

if (failures.length > 0) {
  console.error("Frontend HTTP contract verification failed:")
  for (const failure of failures) console.error(`- ${failure}`)
  process.exit(1)
}

console.log(
  `Frontend HTTP contract verified: ${rustEndpoints.size} Rust endpoints and request schemas match ${path.relative(root, frontendContractPath)}.`,
)
