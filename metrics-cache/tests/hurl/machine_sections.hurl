# The /update_machine_sections endpoint should require authentication.
POST {{baseurl}}/update_machine_sections
Content-Type: application/json
{}
HTTP 401
# TODO
#[Asserts]
#header "Content-Type" == "application/json"

# With a read-only token, it should fail
POST {{baseurl}}/update_machine_sections
Authorization: Bearer {{read_token}}
Content-Type: application/json
file,fixtures/machine_sections_from_python_1.json;
HTTP 401

# Before a successful POST, it should be an empty list
# This also acts as a sanity check, making sure the tests run on a fresh cache.
GET {{baseurl}}/machine_sections
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[]

# Successful POST
POST {{baseurl}}/update_machine_sections
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/machine_sections_from_python_1.json;
HTTP 200
"ok"

# After a successful POST, it should have data
GET {{baseurl}}/machine_sections
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$" count == 1
jsonpath "$[0].node_name" == "kind-worker"

# POSTing the same data again should be fine...
POST {{baseurl}}/update_machine_sections
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/machine_sections_from_python_1.json;
HTTP 200
"ok"

# ...but it should not increase the count.
GET {{baseurl}}/machine_sections
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$" count == 1
jsonpath "$[0].node_name" == "kind-worker"

# But, POSTing *new* data...
POST {{baseurl}}/update_machine_sections
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/machine_sections_from_python_2.json;
HTTP 200
"ok"

# ...should *definitely* increase the count.
GET {{baseurl}}/machine_sections
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$" count == 2
jsonpath "$[*].node_name" contains "kind-worker"
jsonpath "$[*].node_name" contains "unkind-worker"

# And we get metadata for it, too!
GET {{baseurl}}/metadata
Authorization: Bearer {{read_token}}
Content-Type: application/json
HTTP 200
[Asserts]
jsonpath "$.node_collector_metadata[?(@.collector_type == 'Machine Sections')]" count == 2
