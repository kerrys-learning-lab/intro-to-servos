ARG PROJECT=pca9685-service

# ----------------------------------------------------------------------------
FROM rust:1.67-bullseye as base

# https://bobcares.com/blog/debian_frontendnoninteractive-docker/
ARG DEBIAN_FRONTEND=noninteractive

SHELL ["/bin/bash", "-c"]

RUN apt-get update && \
    apt-get upgrade -y && \
    apt-get install -y  i2c-tools \
                        libi2c0 \
                        libi2c-dev

# ----------------------------------------------------------------------------
FROM base as build
ARG PROJECT

# Empty project, to build dependencies ONCE
RUN cd /opt && \
    cargo new --bin ${PROJECT}

WORKDIR /opt/${PROJECT}

COPY Cargo.* ./

# NOTE: With emulation (e.g. --platform linux/arm64), this layer can take
#       upwards of ** 30 MINUTES **
#
#       It's important to build this layer separately from the code, as it will
#       only need to be rebuilt if the dependencies change
RUN cargo build --release && \
    rm -rf ./src

COPY src ./src/
RUN cargo build --release

# ----------------------------------------------------------------------------
FROM base as release
ARG PROJECT

COPY --from=build /opt/${PROJECT}/target/release/${PROJECT} /bin/
ENTRYPOINT [ "/bin/pca9685-service" ]
