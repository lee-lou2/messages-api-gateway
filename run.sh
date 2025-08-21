docker stop messages-api-gateway;docker rm messages-api-gateway;docker run --env-file .env --name=messages-api-gateway -d leelou2/messages-api-gateway:0.0.1
