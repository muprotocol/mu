export default function endpoint(): string {
    return process.env.NEXT_PUBLIC_API as string;
}