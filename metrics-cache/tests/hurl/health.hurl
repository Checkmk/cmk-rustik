# The /health endpoint should be unauthenticated.
GET {{baseurl}}/health
HTTP 200
[Asserts]
header "Content-Type" == "application/json"
{"status":"available"}
