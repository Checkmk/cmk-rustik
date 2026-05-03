# The /update_container_metrics endpoint should require authentication.
POST {{baseurl}}/update_container_metrics
Content-Type: application/json
{}
HTTP 401
# TODO
#[Asserts]
#header "Content-Type" == "application/json"

# With a read-only token, it should fail
POST {{baseurl}}/update_container_metrics
Authorization: Bearer {{read_token}}
Content-Type: application/json
file,fixtures/container_metrics_from_python_1.json;
HTTP 401

# Before a successful POST, it should be an empty list
# This also acts as a sanity check, making sure the tests run on a fresh cache.
GET {{baseurl}}/container_metrics
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[]

# Successful POST
POST {{baseurl}}/update_container_metrics
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/container_metrics_from_python_1.json;
HTTP 200
"ok"

# After a successful POST, it should have data
GET {{baseurl}}/container_metrics
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$" count == 305
jsonpath "$[*].namespace" contains "kube-system"
jsonpath "$[*].namespace" contains "checkmk-monitoring"

# POSTing the same data again should be fine...
POST {{baseurl}}/update_container_metrics
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/container_metrics_from_python_1.json;
HTTP 200
"ok"

# ...but it should not increase the count.
GET {{baseurl}}/container_metrics
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$" count == 305
jsonpath "$[*].namespace" contains "kube-system"
jsonpath "$[*].namespace" contains "checkmk-monitoring"

# But, POSTing *new* data...
POST {{baseurl}}/update_container_metrics
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/container_metrics_from_python_2.json;
HTTP 200
"ok"

# ...should *definitely* increase the count.
GET {{baseurl}}/container_metrics
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$" count == 610
jsonpath "$[*].namespace" contains "kube-system"
jsonpath "$[*].namespace" contains "checkmk-monitoring"

# And we get metadata for it, too!
GET {{baseurl}}/metadata
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$.node_collector_metadata[?(@.collector_type == 'Container Metrics')]" count == 2
