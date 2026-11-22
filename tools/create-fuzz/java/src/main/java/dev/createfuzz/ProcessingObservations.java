package dev.createfuzz;

import com.google.gson.*;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlockEntity;
import com.simibubi.create.content.kinetics.millstone.MillstoneBlockEntity;
import com.simibubi.create.content.kinetics.mixer.MechanicalMixerBlockEntity;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlockEntity;
import com.simibubi.create.content.kinetics.saw.SawBlockEntity;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import com.simibubi.create.content.processing.recipe.ProcessingInventory;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.fluid.SmartFluidTankBehaviour;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.items.IItemHandler;

/** The processing machines' live state, read from their fields without write()'s side effects. */
final class ProcessingObservations {
 static JsonObject observe(BlockEntity be) {
  JsonObject o=new JsonObject();
  if(be instanceof MillstoneBlockEntity m) {
   o.addProperty("kind","millstone");o.addProperty("timer",m.timer);
   o.add("input",slots(m.inputInv));o.add("outputs",slots(m.outputInv));
  } else if(be instanceof CrushingWheelControllerBlockEntity c) {
   o.addProperty("kind","crusher");o.addProperty("speed",c.crushingspeed);o.addProperty("entity",c.hasEntity());
   inventory(o,c.inventory);
  } else if(be instanceof SawBlockEntity s) {
   o.addProperty("kind","saw");inventory(o,s.inventory);
   o.addProperty("recipe_index",TransportObservations.<Integer>field(s,SawBlockEntity.class,"recipeIndex"));
   filter(o,be);
  } else if(be instanceof BasinBlockEntity b) {
   o.addProperty("kind","basin");
   var invs=b.getInvs();o.add("input",slots(invs.getFirst()));o.add("outputs",slots(invs.getSecond()));
   var tanks=b.getTanks();o.add("input_tanks",tanks(tanks.getFirst()));o.add("output_tanks",tanks(tanks.getSecond()));
   filter(o,be);
  } else if(be instanceof MechanicalMixerBlockEntity m) {
   o.addProperty("kind","mixer");o.addProperty("running",m.running);o.addProperty("running_ticks",m.runningTicks);
   o.addProperty("processing_ticks",m.processingTicks);
  } else if(be instanceof MechanicalPressBlockEntity p) {
   var b=p.getPressingBehaviour();
   o.addProperty("kind","press");o.addProperty("running",b.running);o.addProperty("running_ticks",b.runningTicks);
   o.addProperty("mode",b.mode.name().toLowerCase(java.util.Locale.ROOT));
  } else return null;
  return o;
 }
 /** ProcessingInventory: its slots, remainingTime, recipeDuration and appliedRecipe. */
 static void inventory(JsonObject o,ProcessingInventory inv) {
  o.add("slots",slots(inv));o.addProperty("remaining",inv.remainingTime);o.addProperty("duration",inv.recipeDuration);
  o.addProperty("applied",inv.appliedRecipe);
 }
 static void filter(JsonObject o,BlockEntity be) {
  var f=BlockEntityBehaviour.get(be,FilteringBehaviour.TYPE);
  o.add("filter",f==null||f.getFilter().isEmpty()?JsonNull.INSTANCE:new JsonPrimitive(BuiltInRegistries.ITEM.getKey(f.getFilter().getItem()).toString()));
 }
 /** The non-empty slots, with their index. */
 static JsonArray slots(IItemHandler h) {
  JsonArray out=new JsonArray();
  for(int i=0;i<h.getSlots();i++) {
   var s=h.getStackInSlot(i);if(s.isEmpty()) continue;
   JsonObject e=ItemInputs.stack(s).getAsJsonObject();e.addProperty("slot",i);out.add(e);
  }
  return out;
 }
 static JsonArray tanks(SmartFluidTankBehaviour t) {
  JsonArray out=new JsonArray();
  var h=t.getCapability();
  for(int i=0;i<h.getTanks();i++) {
   var fluid=h.getFluidInTank(i);
   if(fluid.isEmpty()) {out.add(JsonNull.INSTANCE);continue;}
   JsonObject e=new JsonObject();e.addProperty("fluid",FluidIds.of(fluid));e.addProperty("amount",fluid.getAmount());out.add(e);
  }
  return out;
 }
}
