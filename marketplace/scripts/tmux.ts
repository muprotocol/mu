import util from './util';

export class TmuxSession {
    private name: string;

    constructor(sessionName: string, command: string) {
        this.name = sessionName;
        util.run(`tmux new-session -d -s ${this.name} '${command}'`);
    }

    newWindow(command: string) {
        util.run(`tmux new-window -d -t ${this.name} '${command}'`);
    }

    splitWindow(command: string, window: string | number, horizontal: boolean) {
        util.run(`tmux split-window -t ${this.name}:${window} ${horizontal ? "-h" : "-v"} '${command}'`);
    }

    attach() {
        util.run(`tmux attach -t ${this.name}`);
    }
}