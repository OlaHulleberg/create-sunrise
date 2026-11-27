package dev.createfuzz;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.deployer.DeployerFakePlayer;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.ItemStack;
import org.apache.commons.lang3.tuple.Pair;

import java.util.Locale;
import java.util.UUID;

/** A deployer and its fake player (`DeployerBlockEntity`, `DeployerFakePlayer`), read from their fields. */
final class DeployerObservations {
    static JsonElement held(ItemStack s) {
        if (s.isEmpty()) return JsonNull.INSTANCE;
        JsonObject o = ItemInputs.stack(s).getAsJsonObject();
        if (s.getDamageValue() != 0) o.addProperty("damage", s.getDamageValue());
        return o;
    }

    static JsonObject observe(DeployerBlockEntity be, BlockPos origin) {
        JsonObject o = new JsonObject();
        o.addProperty("kind", "deployer");
        Enum<?> state = TransportObservations.field(be, DeployerBlockEntity.class, "state");
        Enum<?> mode = TransportObservations.field(be, DeployerBlockEntity.class, "mode");
        o.addProperty("state", state.name().toLowerCase(Locale.ROOT));
        o.addProperty("mode", mode.name().toLowerCase(Locale.ROOT));
        o.addProperty("timer", (int) TransportObservations.field(be, DeployerBlockEntity.class, "timer"));
        o.addProperty("reach", (float) TransportObservations.field(be, DeployerBlockEntity.class, "reach"));
        DeployerFakePlayer player = TransportObservations.field(be, DeployerBlockEntity.class, "player");
        o.add("held", player == null ? JsonNull.INSTANCE : held(player.getMainHandItem()));
        JsonArray inventory = new JsonArray();
        if (player != null) {
            // Its selected slot, the fake player's hand, is the first.
            for (int i = 1; i < player.getInventory().getContainerSize(); i++) {
                ItemStack s = player.getInventory().getItem(i);
                if (!s.isEmpty()) inventory.add(held(s));
            }
        }
        o.add("inventory", inventory);
        JsonArray overflow = new JsonArray();
        for (ItemStack s : TransportObservations.<java.util.List<ItemStack>>field(be, DeployerBlockEntity.class, "overflowItems"))
            overflow.add(held(s));
        o.add("overflow", overflow);
        o.addProperty("locked", (boolean) TransportObservations.field(be, DeployerBlockEntity.class, "redstoneLocked"));
        UUID owner = TransportObservations.field(be, DeployerBlockEntity.class, "owner");
        o.add("owner", owner == null ? JsonNull.INSTANCE : new JsonPrimitive("player"));
        var filtering = BlockEntityBehaviour.get(be, FilteringBehaviour.TYPE);
        o.add("filter", filtering == null || filtering.getFilter().isEmpty() ? JsonNull.INSTANCE
                : new JsonPrimitive(BuiltInRegistries.ITEM.getKey(filtering.getFilter().getItem()).toString()));
        o.addProperty("fist_bump", (boolean) TransportObservations.field(be, DeployerBlockEntity.class, "fistBump"));
        Pair<BlockPos, Float> breaking = player == null ? null
                : TransportObservations.field(player, DeployerFakePlayer.class, "blockBreakingProgress");
        if (breaking == null) o.add("breaking", JsonNull.INSTANCE);
        else {
            JsonObject b = new JsonObject();
            BlockPos p = breaking.getKey().subtract(origin);
            b.add("pos", CreateFuzz.GSON.toJsonTree(new int[]{p.getX(), p.getY(), p.getZ()}));
            b.addProperty("progress", breaking.getValue());
            o.add("breaking", b);
        }
        return o;
    }
}
