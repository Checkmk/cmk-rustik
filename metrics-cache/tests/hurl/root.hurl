# The root endpoint should turn away unauthorized requests.
GET {{baseurl}}/
HTTP 401

# The root endpoint should turn away badly authorized requests.
GET {{baseurl}}/
Authorization: Bearer nope
HTTP 401

# The root endpoint should allow properly authorized requests.
GET {{baseurl}}/
Authorization: Bearer {{read_token}}
HTTP 200

GET {{baseurl}}/
Authorization: Bearer {{write_token}}
HTTP 200
