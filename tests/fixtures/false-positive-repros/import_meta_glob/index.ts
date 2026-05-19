const modules = import.meta.glob("./routes/*.ts", { eager: true });

console.log(Object.keys(modules));
