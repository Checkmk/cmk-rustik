# The general structure of /metadata looks right
GET {{baseurl}}/metadata
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$.cluster_collector_metadata" isObject
jsonpath "$.node_collector_metadata" isList
jsonpath "$.cluster_collector_metadata.node" isString
jsonpath "$.cluster_collector_metadata.host_name" isString
jsonpath "$.cluster_collector_metadata.container_platform.os_name" isString
jsonpath "$.cluster_collector_metadata.container_platform.os_version" isString
jsonpath "$.cluster_collector_metadata.checkmk_kube_agent.project_version" isString
jsonpath "$.cluster_collector_metadata.cache_health.container_metrics.maxsize" == 50000
jsonpath "$.cluster_collector_metadata.cache_health.machine_sections.maxsize" == 50000
