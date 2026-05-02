# The /update_machine_sections endpoint should require authentication.
POST {{baseurl}}/update_machine_sections
Content-Type: application/json
{}
HTTP 401
# TODO
#[Asserts]
#header "Content-Type" == "application/json"

POST {{baseurl}}/update_machine_sections
Authorization: Bearer {{read_token}}
Content-Type: application/json
file,fixtures/machine_sections_from_python.json;
HTTP 401

POST {{baseurl}}/update_machine_sections
Authorization: Bearer {{write_token}}
Content-Type: application/json
file,fixtures/machine_sections_from_python.json;
HTTP 200
