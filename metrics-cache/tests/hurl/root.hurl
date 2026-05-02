# The root endpoint should turn away unauthorized requests.
GET {{baseurl}}/
HTTP 401

# The root endpoint should turn away badly authorized requests.
GET {{baseurl}}/
Authorization: Bearer nope
HTTP 401

# The root endpoint should allow properly authorized requests.
GET {{baseurl}}/
Authorization: Bearer {{token}}
HTTP 200
