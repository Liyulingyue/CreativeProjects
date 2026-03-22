# Frontend build stage
FROM node:20-slim AS build-stage

WORKDIR /app

# Set NPM to Taobao/Tuna mirror
RUN npm config set registry https://registry.npmmirror.com

COPY package*.json ./
RUN npm install

COPY . ./
RUN npm run build

# Production stage
FROM nginx:stable-alpine AS production-stage
COPY --from=build-stage /app/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
