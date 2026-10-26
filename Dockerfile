FROM node:22-alpine
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev
COPY src ./src
ENV DUMMYPLUG_HOST=0.0.0.0
ENV DUMMYPLUG_PORT=9998
EXPOSE 9998
USER node
CMD ["npm", "start"]
