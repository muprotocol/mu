FROM ubuntu:20.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update
RUN apt-get install -y build-essential curl libssl-dev libudev-dev pkg-config zlib1g-dev llvm clang cmake make git zsh language-pack-en sudo netcat
RUN useradd vscode; mkdir /home/vscode; chown vscode.vscode /home/vscode

ENV SOLANA_VERSION 1.11.10
ENV ANCHOR_VERSION 0.25.0

USER vscode

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh /dev/stdin -y
RUN sh -c "$(curl -sSfL https://release.solana.com/v${SOLANA_VERSION}/install)"
RUN /home/vscode/.cargo/bin/cargo install --git https://github.com/project-serum/anchor --tag v${ANCHOR_VERSION} anchor-cli --locked
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.1/install.sh | bash
RUN bash -c "source /home/vscode/.nvm/nvm.sh ; nvm install 16; npm install -g yarn"
RUN echo "export PATH=\$PATH:/home/vscode/.local/share/solana/install/releases/${SOLANA_VERSION}/solana-release/bin/" >> /home/vscode/.bashrc
RUN sh -c "$(curl -fsSL https://raw.github.com/ohmyzsh/ohmyzsh/master/tools/install.sh)"
COPY .zshrc /home/vscode
