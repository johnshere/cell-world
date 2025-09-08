import Entity from './entity';

export default {
  entities: [] as Entity[],
  lastSpawnTime: 0,
  spawnInterval: 1000, // 1秒 = 1000毫秒
  update(deltaTime: number) {
    // deltaTime: 自上次更新以来的毫秒数
    this.lastSpawnTime += deltaTime;

    // 每隔一秒诞生一个新物体
    if (this.lastSpawnTime >= this.spawnInterval) {
      this.entities.push(new Entity());
      this.lastSpawnTime = 0;
    }

    // 更新所有实体
    this.entities.forEach(entity => entity.update(deltaTime));
  },
  render(deltaTime: number) {
    // deltaTime: 自上次渲染以来的毫秒数
    // 渲染所有实体
    this.entities.forEach(entity => entity.render(deltaTime));
  },
};
